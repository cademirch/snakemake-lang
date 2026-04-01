SAMPLES = ["A", "B"]

if True:
    rule conditional:
        input: "data/{sample}.fq"
        output: "qc/{sample}_fastqc.html"
        shell: "fastqc {input} -o qc/"

rule always_runs:
    input: "final_input.txt"
    output: "final_output.txt"
    shell: "finalize {input} > {output}"
