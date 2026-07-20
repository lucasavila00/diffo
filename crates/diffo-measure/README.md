# diffo-measure

`diffo-measure` is the Linux-only performance measurement harness for Diffo.

It runs deterministic pseudo-terminal workloads and reports CPU and text-readiness
measurements used by the performance ADRs. `make measure-startup` separately reports
time to the first terminal output and usable repository frame for fixed mock and
real-Git stress workloads. This developer tool is not published.
