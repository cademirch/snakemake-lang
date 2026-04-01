import os

SAMPLES = ["A", "B", "C"]

rule all:
    input: expand("results/{sample}.txt", sample=SAMPLES)

def get_input(wildcards):
    return f"data/{wildcards.sample}.csv"

rule process:
    input: get_input
    output: "results/{sample}.txt"
    threads: 4
    shell: "process --threads {threads} {input} > {output}"
