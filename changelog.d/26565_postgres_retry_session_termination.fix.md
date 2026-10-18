Fix an issue in the `postgres` sink where batches were dropped instead of retried when PostgreSQL terminated a session (SQLSTATE 57P01, 57P02, 57P03), encountered connection exceptions (class 08), or deadlocked (40001, 40P01).

authors: Tyagiquamar
