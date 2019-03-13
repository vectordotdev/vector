# Vector

## Development

### Sample Logs

We use `flog` to build a sample set of log files to test sending logs from a file. This can
be done with the following commands on mac with homebrew.

``` bash
brew tap mingrammer/flog
brew install flog
$ flog --bytes $((100 * 1024 * 1024)) > sample.log
```

This will create a `100MB` sample log file in the `sample.log` file.

### Building

Building is very easy, all you need to do is use `cargo`:

``` bash
cargo build
```


### Testing

Testing is a bit more complicated, this because to test all the sinks we need to stand
up local mock versions of the sources we send logs too. To do this we use `docker` and 
`docker-compose` to stand up this environment. To run the full test suit you can run

```bash
# Test everything that does not require docker
cargo test

# Test everything that can also be tested with docker
cargo test --features docker
```

### Code Style

We use `rustfmt` on `stable` to format our code and CI will verify that your code follows
this format style. To run the following command make sure `rustfmt` has been installed on
the stable toolchain locally.

``` bash
cargo fmt
```

Once that's all passing and you're happy with your change, go ahead and commit.
For small, unobtrusive changes, committing to directly to master is fine. For
anything that merits more discussion or visibility, committing to a branch and
opening a pull request is preferred. Just use your best judgement and if you're
unsure, open a pull request.
