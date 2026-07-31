Fix Azure Blob Storage uploads larger than 4 MiB when using an account-key connection string. These uploads could send all data blocks successfully but fail with a 403 while completing the upload because the final request's body length was missing during Shared Key signing. Vector now sets the body length before signing that request.

authors: ArunPiduguDD
