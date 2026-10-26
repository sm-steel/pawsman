# `net::protocol`

The server's answer was not valid HTTP, or it closed the connection mid-response.

## When it happens

The server crashed or closed the connection early, sent malformed headers, or speaks another protocol on that port.

## How to fix it

Check the server's logs; send again once it answers properly.

## Related

- [`net::connect`](connect.md)
