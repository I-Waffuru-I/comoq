

default:
   just --list

install:
   cargo install moq-relay

[parallel]
dev: serve site

relay *args:
	TOKIO_CONSOLE_BIND=127.0.0.1:6680 moq-relay dev/relay.toml {{args}}

serve relay_url="http://0.0.0.0:4443": 
   cd rs && cargo run -- --url {{relay_url}}

site:
   cd site && npm run dev




