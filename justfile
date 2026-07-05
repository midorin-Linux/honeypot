# For Windows users. If you are linux or macOS user, please delete this line.
set shell := ["powershell.exe", "-c"]

DB_URL := "sqlite:honeypot.db"

help:
    just -l

fmt:
    cargo +nightly fmt --all

migrate-add name:
    sqlx migrate add {{name}}

migrate-run:
    sqlx migrate run --database-url "{{DB_URL}}"

db-create:
    sqlx db create --database-url "{{DB_URL}}"

db-reset:
    sqlx db reset --database-url "{{DB_URL}}"