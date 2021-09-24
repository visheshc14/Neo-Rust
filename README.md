# `Neo`

`Neo` is a single file server. It responds to every `GET` request it receives with the content of a given file (specified by ENV, CLI argument or STDIN), and for every other request (with any other HTTP method or path) it returns a 404.

`Neo` was invented to help with cases where a generally small file needs to be delivered at a certain path, for example [MTA STS's `/.well-known/mta-sts.txt`](https://en.wikipedia.org/wiki/MTA-STS). 


See also [`-go`](https://github.com/visheshc14/Neo-Go)

# Quickstart

`Neo` only needs the path to a single file to run:

```console
$ Neo -f <file path>
```

By default, `Neo` will serve the file at host `127.0.0.1` on port `5000`. `Neo` can also take file content from STDIN like so:

```console
$ Neo <<EOF
> your file content
> goes here
> EOF
```

# Usage

```console
Neo 0.1.0

USAGE:
    Neo [OPTIONS]

FLAGS:
        --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -f, --file <FILE>                                                File to read [env: FILE=]
    -h, --host <host>                                                Host [env: HOST=]  [default: 127.0.0.1]
    -p, --port <port>                                                Port [env: PORT=]  [default: 5000]
        --stdin-read-timeout-seconds <stdin-read-timeout-seconds>
            Amount of seconds to wait for input on STDIN to serve [env: STDIN_READ_TIMEOUT_SECONDS=]  [default: 60]
```

# Environment Variables

| ENV variable                 | Default     | Example                 | Description                                      |
|------------------------------|-------------|-------------------------|--------------------------------------------------|
| `HOST`                       | `127.0.0.1` | `0.0.0.0`               | The host on which `Neo` will listen             |
| `PORT`                       | `5000`      | `3000`                  | The port on which `Neo` will listen             |
| `STDIN_READ_TIMEOUT_SECONDS` | `60`        | `10`                    | The amount of seconds to try and read from STDIN |
| `FILE`                       | N/A         | `/path/to/your/file`    | The path to the file that will be served         |
| `TLS_KEY`                    | N/A         | `/path/to/your/tls.key` | Path to a file contains a PEM-encoded TLS key    |
| `TLS_CERT`                   | N/A         | `/path/to/your/tls.crt` | Path to a file contains CA cert(s)               |

# Useful Makefile targets

The following targets are mostly useful for development, testing the current build, as most depend on `cargo run`.

## Running the current build of `Neo` with HEREDOCs

```console
$ make run <<EOF
>
> You enter some text
> EOF
cargo run
    Finished dev [unoptimized + debuginfo] target(s) in 0.04s
     Running `target/debug/Neo`
[2021-05-09T02:51:58Z INFO  Neo] Server configured to run @ [127.0.0.1:5000]
[2021-05-09T02:51:58Z INFO  Neo] No file path provided, waiting for input on STDIN (max 60 seconds)...
[2021-05-09T02:51:58Z INFO  Neo] Successfully read input from STDIN
[2021-05-09T02:51:58Z INFO  Neo] Read [16] characters
[2021-05-09T02:51:58Z INFO  Neo] Starting HTTP server...
```

## Serve the example file in the repository

```console
$ make example
```

You can serve the example file in the repository with TLS as well

```console
$ make example-tls
```

Note that the example page is *NOT* included in the `Neo` binary, you have to bring your own file to serve at production time.

## Cutting a release

To cut a release, in a branch or off of `main` do the following:

1. Make necessary code changes (if any) & test
2. Update `Cargo.toml`
3. Update `CHANGELOG.md`
4. Run `make image image-publish image-release`
5. If satisfied, run `make release-prep` (this will create an all-in-one commit that is tagged properly with the new version)
6. `git push` the commit
7. Run `cargo publish` to publish to cargo

# FAQ

# Alternatives

## `miniserve`

**tl;dr `Neo` is about 2x faster than `miniserve`, which is expected as it does much less.**

[`miniserve`](https://crates.io/crates/miniserve) is a project that aims to serve files and directories over HTTP that was suggested. Since `miniserve` is also capable of serving a single file I've tested it gainst `Neo` with the usual `wrk` command and here are the results tabulated. Roughly by running the following:

```console
$ Neo -f /tmp/testfiles/file-1M &
$ miniserve /tmp/testfiles/file-1M -p 5001 &
$ wrk -t12 -c400 -d30s --latency http://127.0.0.1:5000/any/path/will/work # a few times
$ wrk -t12 -c400 -d30s --latency http://127.0.0.1:5001 # a few times
```

|                                    | `Neo` | `miniserve` |
|------------------------------------|--------|-------------|
| Latency 50% (ms)                   | 21.09  | 83.94       |
| Latency 75% (ms)                   | 31.04  | 95.73       |
| Latency 90% (ms)                   | 40.81  | 107.23      |
| Latency 99% (ms)                   | 58.54  | 129.38      |
| (Thread stats) Latency avg (ms)    | 23.29  | 84.94       |
| (Thread stats) Latency stddev (ms) | 12.57  | 17.11       |
| (Thread stats) Latency max (ms)    | 120.18 | 182.87      |
| Request/sec                        | 11,900 | 4599        |
| Trasfer/sec (MB)                   | 1162   | 450         |

As you might expect, `Neo` does so little (though there are some feature differences in the overlap) that it performs roughly 2x as well as `miniserve`.

I did not limit the processes and my machine is pretty beefy: 6 physical cores (12 hyper threads) and 32GB of RAM so these processes got as much room as they cared to use.
