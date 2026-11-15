# Sanitized fixtures

Synthetic, non-production examples based on documented Sentry Integration Platform issue/API shapes. IDs, counts, dates and releases are invented for tests. Explicit OMIT fields exercise allowlist exclusion. No customer history, real credentials or personal information is included. Injected transports and a deliberately absent API credential keep ordinary tests offline.

`checkout-qualification.json` adds an invented checkout-service request contract: expected unauthenticated rejection and a separate submit failure/duplicate observation. Tests explicitly attribute manual replay claims as operator reports; fake results or status codes alone cannot establish expected behavior. This is not the real sanitized service history required for final M3 acceptance.
