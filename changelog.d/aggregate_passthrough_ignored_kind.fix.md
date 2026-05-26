The `aggregate` transform now correctly passes through/ignores metrics whose kind is not supported
by the configured mode. Prior to this change these metrics would be silently dropped, contrary to
the officially documented behavior. For example, `absolute` metrics flowing through a `sum`-mode aggregate
transform are now forwarded to the next step in the pipeline unchanged rather than being dropped:

```text
{kind: incremental, type: counter, name: "http.requests", value: 10}  → summed into aggregate
{kind: absolute,    type: gauge,   name: "cpu.usage",     value: 0.83} → previously dropped, now passes through unchanged
{kind: incremental, type: counter, name: "http.requests", value: 5}   → summed into aggregate
```

If the previous drop behavior was intentional, add a `filter` transform before the aggregate transform to discard the unwanted metric kind.

authors: ArunPiduguDD
