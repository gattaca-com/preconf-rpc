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
beacon-nodes = ["beacon-url-1", "beacon-url-2"]

[[lookahead]]
chain-id = 1
relays = ["relay-1", "relay-2"]

[[lookahead]]
url-provider = "lookahead"
chain-id = 2
relays = ["relay-3"]
[[lookahead.registry]]
"0x8248efd1f054fcccd090879c4011ed91ee9f9d0db5ad125ae1af74fdd33de809ddc882400d99b5184ca065d4570df8cc"  = "http://a-preconfer-url.xyz"
```

### Details
- url-provider: Specifies the source of the URL. It can be either lookahead or url-mapping. 
  - If set to **lookahead**, the URL is derived from the lookahead entry. 
  - If set to **url-mapping**, the URL is determined by looking up the public keys between the lookahead entry public key and the map provided in registry.

Make sure to provide the necessary beacon and relay URLs in the configuration file.