```mermaid
sequenceDiagram
  participant RD as Remote Daemon
  participant D as Daemon
  participant S as Shell
  
  S->>D: Connection request to the IP of the remote machine
  D->>RD: Send a connection request to the listening daemon on the remote

  create participant RS as Remote Shell
  RD->>RS: Creates a new headless remote shell. The port of the daemon is passed in as an argument
  RS->>RD: Send the port it is trying to connect from to the RD
  RD->>D: Send the port that is trying to connect from this computer to the daemon
  D<<->>RS: Accept and establish this connection
  RS->>D: Send the port of the connection it will try to establish connection to the shell on
  D->>S: Send the port and IP of the remote shell attempting to connect to this shell
  S<<->>RS: Establish control connection

  Note over S, RS: This control connection can be used to coordinate pipelines to be run between machines,<br/>potentially making more connections to pipe streams etc.

```

## Direct Network Connections

Both programs bind their IPv4 or IPv6 sockets to the unspecified address, so they
listen on all local interfaces rather than only loopback. They still need a
peer-reachable address to advertise; `0.0.0.0` and `::` are bind addresses, not
addresses to enter on the other machine.

1. Start the controller and enter an IP address the remote can route to, such as
  the controller's LAN or VPN address. The controller prints the control
  address and starts listening before asking for the remote's peer address.
2. Start the remote and enter the controller's printed `IP:port`.
3. After connecting, the remote prints its actual source `IP:port`. Enter that
  full address at the controller so it can verify the peer.
4. Run commands from the controller prompt.

The control connection and each command's stdin, stdout, and stderr use separate
TCP ports. The controller advertises the selected IP with dynamically assigned
ports for those stream listeners. Firewalls must allow those inbound ports.
This works best when the hosts have direct routing, such as on the same LAN or
VPN. NAT generally requires port forwarding for both the control port and the
dynamic stream ports; this implementation does not provide NAT traversal or an
authenticated relay.