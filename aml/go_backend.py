"""Standard-library-only Go backend consuming AML IR."""

from __future__ import annotations

import json
from typing import Any

from .ir import program_ir


RUNTIME = r'''type AMLFailure struct { Message string }

func amlValid(value any, expected string) bool {
	if expected == "Any" { return true }
	if strings.HasPrefix(expected, "Optional[") {
		return value == nil || amlValid(value, expected[9:len(expected)-1])
	}
	if strings.HasPrefix(expected, "List[") {
		items, ok := value.([]any)
		if !ok { return false }
		inner := expected[5:len(expected)-1]
		for _, item := range items { if !amlValid(item, inner) { return false } }
		return true
	}
	if fields, ok := amlRecords[expected]; ok {
		record, valid := value.(map[string]any)
		if !valid || len(record) != len(fields) { return false }
		for name, fieldType := range fields {
			field, exists := record[name]
			if !exists || !amlValid(field, fieldType) { return false }
		}
		return true
	}
	switch expected {
	case "Bool": _, ok := value.(bool); return ok
	case "Float": number, ok := value.(float64); return ok && !math.IsNaN(number) && !math.IsInf(number, 0)
	case "Int": _, ok := value.(int64); return ok
	case "Text": _, ok := value.(string); return ok
	}
	return false
}

func amlExpect(place string, value any, expected string) {
	if !amlValid(value, expected) {
		panic(AMLFailure{fmt.Sprintf("type | %s expected %s, got %T", place, expected, value)})
	}
}

func amlBool(value any) bool {
	result, ok := value.(bool)
	if !ok { panic(AMLFailure{fmt.Sprintf("condition expected Bool, got %T", value)}) }
	return result
}

func amlNumber(value any) (float64, bool) {
	switch number := value.(type) {
	case int64: return float64(number), true
	case float64: return number, true
	default: return 0, false
	}
}

func amlBinary(operator string, left any, right any) any {
	if operator == "==" { return reflect.DeepEqual(left, right) }
	if operator == "!=" { return !reflect.DeepEqual(left, right) }
	if operator == "and" { return amlBool(left) && amlBool(right) }
	if operator == "or" { return amlBool(left) || amlBool(right) }
	if operator == "+" {
		if a, ok := left.(string); ok { if b, valid := right.(string); valid { return a + b } }
	}
	if a, ok := left.(int64); ok {
		if b, valid := right.(int64); valid {
			switch operator {
			case "+": return a + b
			case "-": return a - b
			case "*": return a * b
			case "%": return a % b
			}
		}
	}
	a, leftNumber := amlNumber(left)
	b, rightNumber := amlNumber(right)
	if leftNumber && rightNumber {
		switch operator {
		case "+": return a + b
		case "-": return a - b
		case "*": return a * b
		case "/": return a / b
		case "<": return a < b
		case "<=": return a <= b
		case ">": return a > b
		case ">=": return a >= b
		}
	}
	if a, ok := left.(string); ok {
		if b, valid := right.(string); valid {
			switch operator {
			case "<": return a < b
			case "<=": return a <= b
			case ">": return a > b
			case ">=": return a >= b
			}
		}
	}
	panic(AMLFailure{fmt.Sprintf("operator %s does not support %T and %T", operator, left, right)})
}

func amlUnary(operator string, value any) any {
	if operator == "not" { return !amlBool(value) }
	if number, ok := value.(int64); ok {
		if operator == "-" { return -number }
		if operator == "+" { return number }
	}
	if number, ok := value.(float64); ok {
		if operator == "-" { return -number }
		if operator == "+" { return number }
	}
	panic(AMLFailure{fmt.Sprintf("unary %s does not support %T", operator, value)})
}

func amlIndex(value any, index any) any {
	if items, ok := value.([]any); ok {
		position, valid := index.(int64)
		if !valid || position < 0 || position >= int64(len(items)) { panic(AMLFailure{"invalid list index"}) }
		return items[position]
	}
	if record, ok := value.(map[string]any); ok {
		name, valid := index.(string)
		if !valid { panic(AMLFailure{"record index must be Text"}) }
		result, exists := record[name]
		if !exists { panic(AMLFailure{"unknown record field: " + name}) }
		return result
	}
	panic(AMLFailure{fmt.Sprintf("cannot index %T", value)})
}

func amlRequire(condition any, task string, kind string) {
	if !amlBool(condition) { panic(AMLFailure{fmt.Sprintf("contract | %s %s failed", task, kind)}) }
}'''


def _go_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def _expr(value: dict[str, Any], tasks: set[str]) -> str:
    kind = value["kind"]
    if kind == "literal":
        literal = value["value"]
        if literal is None:
            return "nil"
        if literal is True:
            return "true"
        if literal is False:
            return "false"
        if isinstance(literal, int):
            return f"int64({literal})"
        if isinstance(literal, float):
            return f"float64({literal!r})"
        return _go_string(literal)
    if kind == "name":
        name = value["value"]
        return f"amlTask_{name}" if name in tasks else name
    if kind == "unary":
        return f"amlUnary({_go_string(value['operator'])}, {_expr(value['value'], tasks)})"
    if kind == "binary":
        return f"amlBinary({_go_string(value['operator'])}, {_expr(value['left'], tasks)}, {_expr(value['right'], tasks)})"
    if kind == "call":
        arguments = ", ".join(_expr(item, tasks) for item in value["arguments"])
        return f"{_expr(value['callee'], tasks)}({arguments})"
    if kind == "index":
        return f"amlIndex({_expr(value['value'], tasks)}, {_expr(value['index'], tasks)})"
    if kind == "member":
        return f"amlIndex({_expr(value['value'], tasks)}, {_go_string(value['name'])})"
    if kind == "list":
        return "[]any{" + ", ".join(_expr(item, tasks) for item in value["items"]) + "}"
    if kind == "record":
        fields = ", ".join(
            f"{_go_string(item['name'])}: {_expr(item['value'], tasks)}" for item in value["fields"]
        )
        return "map[string]any{" + fields + "}"
    raise ValueError(f"unsupported IR expression: {kind}")


def _body(
    statements: list[dict[str, Any]],
    task: str,
    return_type: str | None,
    tasks: set[str],
    level: int = 1,
    ensures: list[dict[str, Any]] | None = None,
) -> list[str]:
    indent = "\t" * level
    output: list[str] = []
    if ensures is None:
        ensures = [item["expression"] for item in statements if item["kind"] == "ensure"]
    for statement in statements:
        kind = statement["kind"]
        if kind == "ensure":
            continue
        if kind == "let":
            output.append(f"{indent}{statement['name']} := {_expr(statement['value'], tasks)}")
        elif kind == "show":
            output.append(f"{indent}fmt.Println({_expr(statement['expression'], tasks)})")
        elif kind == "fail":
            output.append(f"{indent}panic(AMLFailure{{fmt.Sprint({_expr(statement['expression'], tasks)})}})")
        elif kind == "require":
            output.append(f"{indent}amlRequire({_expr(statement['expression'], tasks)}, {_go_string(task)}, \"require\")")
        elif kind == "return":
            if statement["value"] is None:
                output.append(f"{indent}return nil")
                continue
            rendered = _expr(statement["value"], tasks)
            if ensures or return_type:
                output.append(f"{indent}result := {rendered}")
                for condition in ensures:
                    output.append(f"{indent}amlRequire({_expr(condition, tasks)}, {_go_string(task)}, \"ensure\")")
                if return_type:
                    output.append(f"{indent}amlExpect({_go_string(task + '.result')}, result, {_go_string(return_type)})")
                output.append(f"{indent}return result")
            else:
                output.append(f"{indent}return {rendered}")
        elif kind == "if":
            output.append(f"{indent}if amlBool({_expr(statement['condition'], tasks)}) {{")
            output.extend(_body(statement["then"], task, return_type, tasks, level + 1, ensures))
            if statement["else"]:
                output.append(f"{indent}}} else {{")
                output.extend(_body(statement["else"], task, return_type, tasks, level + 1, ensures))
            output.append(f"{indent}}}")
        elif kind == "expression":
            output.append(f"{indent}{_expr(statement['expression'], tasks)}")
        else:
            raise ValueError(f"unsupported IR statement: {kind}")
    return output


def compile_go(source: str, filename: str = "<memory>") -> str:
    ir = program_ir(source, filename)
    tasks = {task["name"] for task in ir["tasks"]}
    record_lines = ["var amlRecords = map[string]map[string]string{"]
    for record in ir["records"]:
        fields = ", ".join(
            f"{_go_string(field['name'])}: {_go_string(field['type'])}" for field in record["fields"]
        )
        record_lines.append(f"\t{_go_string(record['name'])}: {{{fields}}},")
    record_lines.append("}")
    output = [
        "// Generated by AML 0.8.0.dev0. Do not edit generated code.",
        "package main",
        "",
        'import ("fmt"; "math"; "os"; "reflect"; "strings")',
        "",
        *record_lines,
        "",
        RUNTIME,
        "",
    ]
    main_seen = False
    for task in ir["tasks"]:
        parameters = ", ".join(f"{item['name']} any" for item in task["parameters"])
        output.append(f"func amlTask_{task['name']}({parameters}) any {{")
        for parameter in task["parameters"]:
            output.append(
                f"\tamlExpect({_go_string(task['name'] + '.' + parameter['name'])}, {parameter['name']}, {_go_string(parameter['type'])})"
            )
        output.extend(_body(task["body"], task["name"], task["return"], tasks))
        output.extend(["\treturn nil", "}", ""])
        main_seen = main_seen or task["name"] == "main"
    if main_seen:
        output.extend(
            [
                "func main() {",
                "\tdefer func() {",
                "\t\tif recovered := recover(); recovered != nil {",
                "\t\t\tif failure, ok := recovered.(AMLFailure); ok { fmt.Fprintln(os.Stderr, \"AML_ERROR:\", failure.Message); os.Exit(1) }",
                "\t\t\tpanic(recovered)",
                "\t\t}",
                "\t}()",
                "\tamlTask_main()",
                "}",
                "",
            ]
        )
    return "\n".join(output)
