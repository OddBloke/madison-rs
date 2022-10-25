# madison-rs

## Quickstart

In the root of this repo, run the Rocket development server:

```
ROCKET_PORT=5000 cargo run
```

(Using a release build makes a huge difference to performance: ~1s vs 15-20s locally, so consider
passing `--release`.)

Once the server is up-and-running, you can point `rmadison` at it:

```
$ rmadison -u http://localhost:5000 systemd
systemd | 237-3ubuntu10     | bionic         | source
systemd | 237-3ubuntu10.56  | bionic-updates | source
systemd | 245.4-4ubuntu3    | focal          | source
systemd | 245.4-4ubuntu3.18 | focal-updates  | source
systemd | 249.11-0ubuntu3   | jammy          | source
systemd | 249.11-0ubuntu3.6 | jammy-updates  | source
systemd | 251.4-1ubuntu7    | kinetic        | source
```
