# preconf-rpc

> :warning: **this repository is under heavy development**


A command-line interface for executing tpreconf-rpc commands.

## Usage

### Running the Forward Service

To execute the forward service, use the following command:

```sh
./preconf-rpc forward --relay-urls <RELAY_URLS> --beacon-urls <BEACON_URLS> [--port <PORT>]
```

#### Arguments

- `--relay-urls`: Space-separated list of relay URLs.
- `--beacon-urls`: Space-separated list of beacon URLs.
- `--port`: (Optional) Port to run the service on (default is 8000).

### Example

```sh
./preconf-rpcforward --relay-urls http://relay1.url http://relay2.url --beacon-urls http://beacon1.url http://beacon2.url --port 8080
```

## Environment Variables

- `RUST_LOG`: Set the logging level (default is `info`). Example: `RUST_LOG=debug`.
