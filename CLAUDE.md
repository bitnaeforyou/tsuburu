## Agent verification

Background-safe checks for this repository (see the global Testing policy):

- `cargo test` and `cargo clippy --all-targets`
- Do not start `docker compose` services without asking; they bind ports and use significant resources.
