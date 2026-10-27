# `model::var_cycle`

Variables refer to each other in a loop, so none of them has a value.

## When it happens

A variable's value may use other variables, for example `api = "{{base}}/v1"`. When the chain comes back to where it started (`a` uses `b`, `b` uses `a`), the value can never be worked out. The message lists the loop in order.

## How to fix it

Change one variable in the loop so it no longer uses the others, usually by writing its value out in full.

## Related

- [`model::unknown_var`](unknown_var.md)
