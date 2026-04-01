rule align:
    input: "reads/{sample}.fastq"
    output: "aligned/{sample}.bam"
    threads: 8
    shell: "bwa mem -t {threads} {input} > {output}"
