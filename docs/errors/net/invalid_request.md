# `net::invalid_request`

The request has a method, URL or header that is not valid HTTP.

## When it happens

The URL has no `http` or `https` scheme or no host, contains spaces, or a header name has spaces or a header value has a line break. Nothing is sent.

## How to fix it

Fix the part the message names in the request file.

## Related

- [`model::invalid_file`](../model/invalid_file.md)
