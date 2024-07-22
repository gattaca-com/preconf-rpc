# preconf-rpc

> :warning: **This repository is under heavy development**

A RPC proxy that forwards preconfirmations requests to preconfers, based on the current lookahead schedule

![](/assets/lookahead.png)

The RPC:
- builds a local lookahead for each chain id (currently only L1 via the `/preconfer` endpoint on Constraints API)
- forwards requests from users to the next lookahed in the schedule

## Usage

### Running the Forward Service

To execute the forward service, use the following command:

```sh
./preconf-rpc forward --config <CONFIG_FILE> [--port <PORT>]
```

#### Arguments

- `--config`: Path to the configuration file containing lookahead providers configuration.
- `--port`: (Optional) Port to run the service on (default is 8000).

### Example

```sh
./preconf-rpc forward --config configuration.toml --port 8080
```

## Environment Variables

- `RUST_LOG`: Set the logging level (default is `info`). Example: `RUST_LOG=debug`.

## Configuration File

Example `configuration.toml`:

```toml
beacon-urls = ["beacon-url-1", "beacon-url-2"]

[[lookahead-providers-relays]]
chain-id = 1
relay-urls = ["relay-1", "relay-2"]

[[lookahead-providers-relays]]
chain-id = 2
relay-urls = ["relay-3"]
```

Make sure to provide the necessary beacon and relay URLs in the configuration file.