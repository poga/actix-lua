# lua-web

This example demostrate building a basic web framework with pure lua. Including:

* a http router
* rendering template with [liluat](https://github.com/FSMaxB/liluat)

## Usage

Start the server:

```
$ cargo run web.lua
```

Test it:

```
$ http get localhost:8080/hello
HTTP/1.1 200 OK
content-encoding: gzip
content-length: 146
date: Sat, 25 Aug 2018 00:57:54 GMT

<!DOCTYPE html>
<html lang="en">
	<head>
		<meta charset="UTF-8">
		<title>hello world</title>
	</head>
	<body>
		<h1>Hello  from Lua!</h1>
	</body>
</html>
```