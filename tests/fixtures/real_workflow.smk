configfile: "config.yaml"

SAMPLES = config["samples"]
GENOME = config["genome"]

rule all:
    input:
        expand("results/counts/{sample}.counts.txt", sample=SAMPLES),
        "results/multiqc_report.html"

rule fastqc:
    input: "data/{sample}.fastq.gz"
    output: "results/fastqc/{sample}_fastqc.html"
    conda: "envs/fastqc.yaml"
    threads: 2
    shell: "fastqc -t {threads} {input} -o results/fastqc/"

rule trim:
    input: "data/{sample}.fastq.gz"
    output: "results/trimmed/{sample}.trimmed.fastq.gz"
    params:
        extra="--quality 20 --length 50"
    log: "logs/trim/{sample}.log"
    conda: "envs/trimgalore.yaml"
    shell: "trim_galore {params.extra} -o results/trimmed/ {input} 2> {log}"

rule align:
    input:
        reads="results/trimmed/{sample}.trimmed.fastq.gz",
        index=GENOME
    output:
        bam="results/aligned/{sample}.bam",
        bai="results/aligned/{sample}.bam.bai"
    threads: 8
    resources:
        mem_mb=16384
    log: "logs/align/{sample}.log"
    conda: "envs/star.yaml"
    shell: "STAR --runThreadN {threads} --genomeDir {input.index} --readFilesIn {input.reads} 2> {log}"

rule count:
    input:
        bam="results/aligned/{sample}.bam",
        gtf=config["gtf"]
    output: "results/counts/{sample}.counts.txt"
    conda: "envs/subread.yaml"
    shell: "featureCounts -a {input.gtf} -o {output} {input.bam}"

rule multiqc:
    input:
        expand("results/fastqc/{sample}_fastqc.html", sample=SAMPLES),
        expand("results/counts/{sample}.counts.txt", sample=SAMPLES)
    output: "results/multiqc_report.html"
    conda: "envs/multiqc.yaml"
    shell: "multiqc results/ -o results/ -n multiqc_report.html"

onsuccess:
    print("RNA-seq analysis complete!")

onerror:
    print("RNA-seq analysis failed. Check logs/")
