rule full_example:
    input:
        reads="data/{sample}.fq",
        ref="genome.fa"
    output:
        bam="aligned/{sample}.bam",
        bai="aligned/{sample}.bam.bai"
    params:
        extra="--rg-id {sample}"
    log: "logs/{sample}.log"
    benchmark: "benchmarks/{sample}.txt"
    threads: 8
    resources:
        mem_mb=4096,
        disk_mb=1000
    retries: 3
    priority: 50
    conda: "envs/align.yaml"
    container: "docker://biocontainers/bwa:0.7.17"
    envmodules: "bwa/0.7.17", "samtools/1.15"
    message: "Aligning {wildcards.sample}"
    wildcard_constraints:
        sample="[A-Za-z0-9]+"
    shadow: "minimal"
    group: "alignment"
    shell: "bwa mem -t {threads} {params.extra} {input.ref} {input.reads} | samtools sort -o {output.bam}"
