# Russh-backed remote connections through `ShellHandle`

This note describes how to evolve the current remote-shell prototype into an
SSH-encrypted transport, while making remote connection and execution
capabilities accessible through `ShellHandle`.

## Where the current design stands

- [`ShellHandle`](crates/shell-core/src/shell.rs#L89) is currently a cloneable
  handle to one local `Shell`; it provides read and write closures over that
  shell at [`shell`](crates/shell-core/src/shell.rs#L94). It has no remote
  connection/session API yet.
- The core stream layer is already transport-flexible:
  [`StreamSender` and `StreamReceiver`](crates/shell-core/src/streams.rs#L12)
  abstract sending and receiving, and [`StreamTx` / `StreamRx`](crates/shell-core/src/streams.rs#L93)
  wrap those traits. Tokio async readers and writers already implement the
  traits, so compatible SSH channel adapters can plug in without making the
  command execution code understand SSH.
- The connection crate currently owns a MessagePack control protocol over a
  [`TcpStream`](crates/shell-connection/src/lib.rs#L12). Its
  [`ControlMessage` and `RemoteMessage`](crates/shell-connection/src/control.rs#L32)
  represent command requests, stream setup, and command completion.
- The headless controller listens for a raw TCP control connection and creates
  three more TCP listeners for stdin, stdout, and stderr in
  [`run_cmd`](crates/shell-headless/src/controller.rs#L86). The remote connects
  those stream sockets and reports their peer addresses in
  [`run_cmd`](crates/shell-headless/src/shell.rs#L105). Neither the existing
  TCP handshake nor those side-channel sockets provide encryption or peer
  authentication.
- `shell-connection` already depends on `shell-core`
  ([manifest](crates/shell-connection/Cargo.toml#L10)). Adding a dependency
  from core back to the connection crate would create a dependency cycle.

## Target architecture

Keep `shell-core` independent of russh, but make `ShellHandle` the owner and
public entry point for both local and remote shells. Core defines the
remote-connection API and transport contracts; `shell-connection` supplies the
russh implementation. A `ShellHandle` stores its connected remotes internally
as other shells it can control. Callers ask the handle to connect, list or
disconnect remotes, and run commands against a selected remote; they do not
take ownership of or retain a russh session object.

Conceptually, the public core API would look like:

```rust,ignore
let shell = Shell::default().into_handle();
shell.set_remote_connector(connector);
let remote_id = shell.connect(target, auth_policy).await?;
let remotes = shell.remotes().await?;
let result = shell.run_remote(remote_id, command, io).await?;
shell.disconnect(remote_id).await?;
```

This is an API sketch, not a proposed exact signature. `RemoteId` is an opaque,
stable identifier rather than a remote/session object. Internally, the handle
can keep a `HashMap<RemoteId, RemoteShell>` for lookup and management; `remotes`
can return lightweight metadata (identifier, name/address, connection status),
not the stored transport. A vector may still be useful when presenting a
stable ordered listing, but should not be the primary lookup structure.

The current [`ShellHandle`](crates/shell-core/src/shell.rs#L89) wraps
`Arc<RwLock<Shell>>`. Evolve that ownership boundary to include a private remote
registry—either in a new handle-state type containing the local shell and the
registry, or as a separate shared registry owned by the handle. Do not put
russh types in the public core state. Clone an internal session/driver handle
out of the registry before awaiting network I/O; never hold a shell or registry
lock while connecting, sending, or waiting for command output. Concurrent
operations should be synchronized per remote/session rather than by holding a
global lock, and disconnecting/removing a remote should close its channels and
release its transport resources.

The core-owned connector and session traits should use transport-neutral types.
The concrete russh connector and its configuration types remain in
`shell-connection`; applications that do not enable remote functionality can
continue using a local `ShellHandle` without depending on russh.

There are two reasonable ways to install the implementation without reversing
the crate dependency:

1. **Inject a connector:** define a `RemoteConnector` trait in core and let
   `shell-connection` implement it for its russh connector. The application
   passes the implementation into the handle. This makes alternate transports
   and test doubles straightforward.
2. **Feature-gated integration crate:** if a concrete convenience method is
   needed, add a small integration crate that depends on both core and
   connection and provides the builder/wiring. Core still owns the abstraction
   and the user-facing operation shape.

Do not put russh-specific configuration, channel types, or host-key callbacks
in `shell-core`; that would undermine transport independence and make a
concrete transport part of every core consumer's API.

## SSH session and channel layout

Establish one authenticated SSH session for a remote shell session. Replace
the current TCP control socket and the three separately advertised TCP ports
with SSH channels on that session. A straightforward first protocol can keep
the existing logical messages (`RunCommand`, stream setup, completion/error),
but carry them over a dedicated control channel. Open separate channels for
stdin, stdout, and stderr per command, or use one command channel with explicit
framing and stream identifiers. Either approach keeps all remote bytes inside
the encrypted SSH connection.

The adapters at the boundary translate russh channel data, EOF/close, and
backpressure into the core's `StreamReceiver`/`StreamSender` behavior. That
lets `CommandContext` and the existing `StreamAdapter` logic work with remote
streams without changes to command implementations. Ensure command completion
and channel closure are distinct and ordered correctly; handle a remote closing
one stream early without hanging the other streams or the control channel.

The existing code lets the remote initiate a TCP connection to a listening
controller. SSH requires choosing which side is the SSH client and which side
hosts the SSH server. Make that role/topology an explicit part of connection
configuration. To preserve a controller-listens workflow, the controller can
host the SSH server and the remote can initiate the SSH client connection; an
alternative deployment can have the controller connect to an SSH server on the
remote. Do not infer authorization just from which side opened the socket.

## Trust and authentication

Encryption protects traffic after SSH key exchange, but the peer identity
still has to be verified. Model peer verification and user authentication as
configurable policy/hooks rather than assuming one fixed credential method.
Support policy implementations for:

- **Third-party-assisted TOFU:** obtain a first-use identity decision from the
  external trust service, persist/compare the accepted peer identity, and
  reject unexpected changes unless an explicit policy approves them.
- **Certificates:** verify the presented SSH certificate, its signature,
  identity/principal, validity interval, and trust authority. If both systems
  have identities, verify each direction required by the deployment.
- **One-time approval:** obtain explicit consent for that connection and bind
  the approval to the intended peer/session. Do not turn a one-time approval
  into persistent trust unless the user or policy explicitly asks for it.

Keep these responsibilities distinct: server/host identity verification
answers “is this the intended SSH server?”, while user/client authentication
answers “may this client log in?”. Unknown, changed, expired, or rejected
identities must fail closed. Authentication must never downgrade to the
current plaintext TCP protocol automatically. If the old transport is retained
temporarily for migration, expose it as a visibly separate, opt-in legacy mode
and do not describe it as secure.

## Migration sequence

1. Define the core transport-neutral remote session/connector and auth-policy
   contracts, add an opaque `RemoteId` and a private remote-shell registry to
   `ShellHandle`, and expose connect/list/run/disconnect operations on the
   handle. A successful connect stores the remote internally and returns only
   its ID/metadata. Avoid holding the shell or registry lock across network
   operations; clone the connector/session driver handle out before awaiting.
2. Implement the russh connector in `shell-connection`. Add channel adapters
   and map the existing logical control messages to an SSH control channel.
   Start with one end-to-end command and its three streams, and ensure the
   handle can manage multiple independent remote entries without serializing
   unrelated remotes behind a global lock.
3. Wire the headless controller and remote binaries through the new
   `ShellHandle` API. Replace raw TCP listeners and peer-address checks with
   SSH setup and configured trust/authentication. Keep command parsing and
   shell execution in core.
4. Add tests for accepted/rejected peer identities, one-time approvals,
   channel EOF and closure, simultaneous stdin/stdout/stderr, command errors,
   and no plaintext fallback. Update CLI help and user documentation to
   explain SSH roles, trust setup, and migration from the prototype.

## Design checks before implementation

- Decide whether the first delivery supports only one SSH role topology or
  both client/server arrangements; represent it explicitly in configuration.
- Decide whether each `ShellHandle` has one configured connector or a registry
  of connector configurations. In either case, connected remote shells remain
  private entries owned by the handle, addressed externally by opaque IDs;
  short-lived command channels are created per operation and are not stored as
  long-lived remote state.
- Decide whether stdin/stdout/stderr use three SSH channels or one framed
  channel. Separate channels are simpler to reason about initially; a
  multiplexed channel reduces channel count but needs a robust framing protocol.
- Define how third-party TOFU policy persists identity pins and handles
  service unavailability. A failed trust lookup should not silently accept a
  peer.
- Keep command execution authorization separate from successful SSH login:
  an authenticated peer should only gain the remote shell capabilities
  explicitly granted to it.
