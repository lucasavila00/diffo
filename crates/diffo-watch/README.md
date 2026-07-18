# diffo-watch

`diffo-watch` provides live repository refresh services for Diffo.

It combines filesystem notifications with background snapshot loading and reports
refresh results to the application. It also brokers typed, operation-scoped prompts and
cancellation outside the worker command queue. Presentation and repository-state
ownership stay outside this crate.
