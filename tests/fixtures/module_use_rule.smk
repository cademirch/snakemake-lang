module qc:
    snakefile: "qc/Snakefile"
    config: config

use rule * from qc exclude trim as qc_*

use rule align from qc with:
    threads: 16
    resources:
        mem_mb=8192
