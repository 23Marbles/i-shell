# Grammar

![Railroad grammar](pipe-grammar-railroad.svg)

This uses a very simple grammar to parse commands and construct pipes.

## Command

![Railroad grammar](command-grammar-railroad.svg)

## Arguments

![Railroad grammar](arg-grammar-railroad.svg)

![Railroad grammar](args-grammar-railroad.svg)

## Location

![Railroad grammar](location-grammar-railroad.svg)

The location is where the command will be executed. It can be a directory and/or Ip. If it is an Ip
a connection will attempt to be established if there does not already exist one.
