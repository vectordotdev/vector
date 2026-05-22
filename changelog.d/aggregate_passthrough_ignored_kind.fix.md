The `aggregate` transform now correctly passes through metrics whose kind is not supported by the configured mode, rather than silently dropping them. For example, `absolute` metrics flowing through a `sum`-mode aggregate are forwarded to the output unchanged without any aggregation & without being dropped

authors: ArunPiduguDD
