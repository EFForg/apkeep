FROM rust:latest

COPY . /app

WORKDIR /app

RUN cargo install --path .

ENTRYPOINT ["apkeep"]
