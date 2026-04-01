configfile: "config.yaml"

include: "rules/align.smk"
include: "rules/call.smk"

workdir: "analysis/"

wildcard_constraints:
    sample="[A-Za-z0-9]+",
    chr="chr[0-9XY]+"

ruleorder: bwa_align > bowtie_align

localrules: all, clean

container: "docker://continuumio/miniconda3:4.8.2"

envvars: "TMPDIR", "HOME"
