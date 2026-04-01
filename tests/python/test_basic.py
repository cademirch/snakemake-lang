"""Basic tests for snakemake-lang Python bindings."""
import pytest
from snakemake_lang import parse_and_compile


def test_simple_rule():
    source = 'rule foo:\n    input: "a.txt"\n    output: "b.txt"\n    shell: "cat {input} > {output}"\n'
    code, linemap = parse_and_compile(source, "test.smk")
    assert isinstance(code, str)
    assert isinstance(linemap, dict)
    assert len(code) > 0
    compile(code, "<test>", "exec")


def test_syntax_error():
    with pytest.raises(SyntaxError) as exc_info:
        parse_and_compile("rule :\n    input: \"x\"\n", "test.smk")
    assert exc_info.value.filename == "test.smk"


def test_empty_source():
    code, linemap = parse_and_compile("", "test.smk")
    assert isinstance(code, str)


def test_python_passthrough():
    source = "x = 1\ny = 2\n"
    code, linemap = parse_and_compile(source, "test.smk")
    assert "x = 1" in code
    assert "y = 2" in code


def test_mixed_workflow():
    source = 'configfile: "config.yaml"\n\nrule all:\n    input: "result.txt"\n\nrule process:\n    input: "data.csv"\n    output: "result.txt"\n    shell: "process {input} > {output}"\n'
    code, linemap = parse_and_compile(source, "test.smk")
    assert len(code) > 0
    compile(code, "<test>", "exec")
