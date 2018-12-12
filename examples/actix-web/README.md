# actix-web lua example

A http server built with `actix-lua` and `actix-web`.

## Usage

Start the server:

```
$ cargo run
```

Test it:

```
$ http get localhost:8080/jack
HTTP/1.1 200 OK
content-length: 10
content-type: application/json
date: Sun, 12 Aug 2018 17:42:51 GMT

"hi! jack"
```