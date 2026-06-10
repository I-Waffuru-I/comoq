

default:
   just --list

install:
	npm install

[parallel]
dev: server site

relay *args:
	TOKIO_CONSOLE_BIND=127.0.0.1:6680 moq-relay dev/relay.toml {{args}}

site:
   cd site && npm run dev

server url="127.0.0.1:4443" cert="../dev/cert.pem" key="../dev/key.pem":
    cd serv && cargo run --bin server -- -u {{url}} -c {{cert}} -k {{key}}

dev-cert ip="192.168.1.62":
    mkdir -p dev
    openssl req -x509 -newkey ec \
        -pkeyopt ec_paramgen_curve:P-256 \
        -keyout dev/key.pem \
        -out dev/cert.pem \
        -days 14 -nodes \
        -subj "/CN=localhost" \
        -addext "subjectAltName=DNS:localhost,IP:127.0.0.1,IP:0.0.0.0,IP:{{ip}}"


latency:
   cd serv && cargo run --bin latency

overload:
   cd serv && cargo run --bin overload 

