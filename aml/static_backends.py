"""Standard-library scalar-core backends for native and managed targets."""

from __future__ import annotations

from dataclasses import dataclass
import json
from typing import Any

from .compiler import CompileError
from .ir import program_ir
from .typesystem import parse_type


SCALARS = {"Bool", "Float", "Int", "Text"}


@dataclass(frozen=True)
class StaticTarget:
    name: str
    types: dict[str, str]
    true: str
    false: str
    void: str
    indent: str


TARGETS = {
    "c": StaticTarget("c", {"Bool": "bool", "Float": "double", "Int": "long long", "Text": "const char *"}, "true", "false", "void", "    "),
    "rust": StaticTarget("rust", {"Bool": "bool", "Float": "f64", "Int": "i64", "Text": "String"}, "true", "false", "()", "    "),
    "java": StaticTarget("java", {"Bool": "boolean", "Float": "double", "Int": "long", "Text": "String"}, "true", "false", "void", "    "),
    "csharp": StaticTarget("csharp", {"Bool": "bool", "Float": "double", "Int": "long", "Text": "string"}, "true", "false", "void", "    "),
    "swift": StaticTarget("swift", {"Bool": "Bool", "Float": "Double", "Int": "Int64", "Text": "String"}, "true", "false", "Void", "    "),
}


class Renderer:
    def __init__(self, target: StaticTarget, ir: dict[str, Any]):
        self.target = target
        self.ir = ir
        self.signatures = {
            task["name"]: ([item["type"] for item in task["parameters"]], task["return"])
            for task in ir["tasks"]
        }
        self.records = {
            record["name"]: {field["name"]: field["type"] for field in record["fields"]}
            for record in ir["records"]
        }

    def validate(self) -> None:
        if self.ir["records"] and self.target.name not in {"rust", "java", "csharp", "swift", "c"}:
            raise CompileError(
                f"{self.target.name} backend currently supports scalar core; records are the next conformance tier",
                self.ir["records"][0]["line"],
            )
        if self.target.name in {"rust", "java", "csharp", "swift", "c"}:
            for record in self.ir["records"]:
                for field in record["fields"]:
                    self.supported_type(field["type"], record["line"])
        for task in self.ir["tasks"]:
            for parameter in task["parameters"]:
                self.supported_type(parameter["type"], task["line"])
            if task["return"]:
                self.supported_type(task["return"], task["line"])
                if not self.definitely_returns(task["body"]):
                    raise CompileError(
                        f"task '{task['name']}' declares a return type but does not return on every path",
                        task["line"],
                    )

    def supported_type(self, value: str, line: int = 1) -> str:
        parsed = parse_type(value)
        if self.target.name in {"rust", "java", "csharp", "swift", "c"}:
            if parsed.name in SCALARS or parsed.name in self.records:
                return value
            if parsed.name in {"List", "Optional"}:
                self.supported_type(parsed.arguments[0].canonical(), line)
                return value
        elif value in SCALARS:
            return value
        if value not in SCALARS:
            raise CompileError(
                f"{self.target.name} scalar-core backend does not yet support {value}", line
            )
        return value

    def target_type(self, aml_type: str) -> str:
        if aml_type in self.target.types:
            return self.target.types[aml_type]
        parsed = parse_type(aml_type)
        if self.target.name == "rust":
            if parsed.name == "List":
                return f"Vec<{self.target_type(parsed.arguments[0].canonical())}>"
            if parsed.name == "Optional":
                return f"Option<{self.target_type(parsed.arguments[0].canonical())}>"
            if parsed.name in self.records:
                return parsed.name
        if self.target.name == "java":
            if parsed.name == "List":
                return f"List<{self.boxed_type(parsed.arguments[0].canonical())}>"
            if parsed.name == "Optional":
                return f"Optional<{self.boxed_type(parsed.arguments[0].canonical())}>"
            if parsed.name in self.records:
                return parsed.name
        if self.target.name == "csharp":
            if parsed.name == "List":
                return f"List<{self.target_type(parsed.arguments[0].canonical())}>"
            if parsed.name == "Optional":
                return f"{self.target_type(parsed.arguments[0].canonical())}?"
            if parsed.name in self.records:
                return parsed.name
        if self.target.name == "swift":
            if parsed.name == "List":
                return f"[{self.target_type(parsed.arguments[0].canonical())}]"
            if parsed.name == "Optional":
                return f"{self.target_type(parsed.arguments[0].canonical())}?"
            if parsed.name in self.records:
                return parsed.name
        if self.target.name == "c":
            if parsed.name == "List":
                return self.c_type_name(aml_type)
            if parsed.name == "Optional":
                return self.c_type_name(aml_type)
            if parsed.name in self.records:
                return parsed.name
        raise CompileError(f"{self.target.name} backend cannot lower type {aml_type}", 1)

    def c_type_name(self, aml_type: str) -> str:
        safe = "".join(character if character.isalnum() else "_" for character in aml_type)
        return f"AML_{safe}"

    def boxed_type(self, aml_type: str) -> str:
        boxes = {"Bool": "Boolean", "Float": "Double", "Int": "Long", "Text": "String"}
        if aml_type in boxes:
            return boxes[aml_type]
        return self.target_type(aml_type)

    def expression_type(self, value: dict[str, Any], env: dict[str, str]) -> str:
        if "type" in value:
            return value["type"]
        kind = value["kind"]
        if kind == "literal":
            literal = value["value"]
            if literal is None: return "None"
            if isinstance(literal, bool): return "Bool"
            if isinstance(literal, int): return "Int"
            if isinstance(literal, float): return "Float"
            if isinstance(literal, str): return "Text"
            raise CompileError(f"{self.target.name} scalar-core backend does not support none", 1)
        if kind == "name":
            if value["value"] not in env:
                raise CompileError(f"cannot infer type of '{value['value']}'", 1)
            return env[value["value"]]
        if kind == "unary":
            return "Bool" if value["operator"] == "not" else self.expression_type(value["value"], env)
        if kind == "binary":
            operator = value["operator"]
            left = self.expression_type(value["left"], env)
            right = self.expression_type(value["right"], env)
            if operator in {"==", "!=", "<", "<=", ">", ">=", "and", "or"}: return "Bool"
            if operator == "+" and left == right == "Text": return "Text"
            if operator == "/" or "Float" in {left, right}: return "Float"
            return "Int"
        if kind == "call":
            callee = value["callee"]
            if callee["kind"] != "name" or callee["value"] not in self.signatures:
                raise CompileError("static backends require calls to named AML tasks", 1)
            result = self.signatures[callee["value"]][1]
            if result is None:
                raise CompileError("a void task cannot be used as a value", 1)
            return result
        raise CompileError(
            f"{self.target.name} scalar-core backend does not support {kind} expressions", 1
        )

    def string(self, value: str) -> str:
        return json.dumps(value, ensure_ascii=False)

    def name(self, value: str) -> str:
        return f"aml_{value}"

    def expr(self, value: dict[str, Any], env: dict[str, str]) -> str:
        kind = value["kind"]
        target = self.target.name
        if kind == "literal":
            literal = value["value"]
            if literal is None and target == "rust": return "None"
            if literal is None and target in {"java", "csharp"}: return "null"
            if literal is None and target == "swift": return "nil"
            if literal is None and target == "c": return "0"
            if literal is True: return self.target.true
            if literal is False: return self.target.false
            if isinstance(literal, str):
                text = self.string(literal)
                return f"{text}.to_string()" if target == "rust" else text
            if isinstance(literal, float):
                return repr(literal)
            if isinstance(literal, int):
                if target == "swift": return f"Int64({literal})"
                if target == "java": return f"{literal}L"
                return str(literal)
            raise CompileError(f"{target} scalar-core backend does not support none", 1)
        if kind == "name":
            return value["value"]
        if kind == "unary":
            operator = "!" if value["operator"] == "not" else value["operator"]
            return f"({operator}{self.expr(value['value'], env)})"
        if kind == "binary":
            operator = value["operator"]
            left_type = self.expression_type(value["left"], env)
            right_type = self.expression_type(value["right"], env)
            left = self.expr(value["left"], env)
            right = self.expr(value["right"], env)
            if left_type.startswith("Optional[") and right_type == "None":
                right = self.coerce(right, right_type, left_type)
            if right_type.startswith("Optional[") and left_type == "None":
                left = self.coerce(left, left_type, right_type)
            if target == "c" and operator in {"==", "!="}:
                if left_type.startswith("Optional[") and right_type == "None":
                    comparison = f"({self.expr(value['left'], env)}).has"
                    return f"(!{comparison})" if operator == "==" else comparison
                if right_type.startswith("Optional[") and left_type == "None":
                    comparison = f"({self.expr(value['right'], env)}).has"
                    return f"(!{comparison})" if operator == "==" else comparison
                if left_type not in SCALARS or right_type not in SCALARS:
                    raise CompileError("C structured equality currently supports Optional[T] versus none", 1)
            numeric_result = "Float" in {left_type, right_type} or operator == "/"
            if numeric_result:
                left = self.coerce(left, left_type, "Float")
                right = self.coerce(right, right_type, "Float")
            if operator == "and": operator = "&&"
            elif operator == "or": operator = "||"
            if target == "c" and left_type == right_type == "Text":
                if operator == "+": return f"aml_text_add({left}, {right})"
                comparisons = {"==": "== 0", "!=": "!= 0", "<": "< 0", "<=": "<= 0", ">": "> 0", ">=": ">= 0"}
                if operator in comparisons: return f"(strcmp({left}, {right}) {comparisons[operator]})"
            if target == "rust" and operator == "+" and left_type == right_type == "Text":
                return f"format!(\"{{}}{{}}\", {left}, {right})"
            if target == "java" and left_type == right_type == "Text":
                if operator == "==": return f"({left}.equals({right}))"
                if operator == "!=": return f"(!{left}.equals({right}))"
                comparisons = {"<": "< 0", "<=": "<= 0", ">": "> 0", ">=": ">= 0"}
                if operator in comparisons: return f"({left}.compareTo({right}) {comparisons[operator]})"
            if target == "java" and operator in {"==", "!="} and (
                left_type not in SCALARS or right_type not in SCALARS or "None" in {left_type, right_type}
            ):
                equality = f"Objects.equals({left}, {right})"
                return f"(!{equality})" if operator == "!=" else equality
            return f"({left} {operator} {right})"
        if kind == "call":
            callee = value["callee"]["value"]
            expected = self.signatures[callee][0]
            arguments = ", ".join(
                self.coerce(self.expr(item, env), self.expression_type(item, env), parameter_type)
                for item, parameter_type in zip(value["arguments"], expected)
            )
            return f"{self.name(callee)}({arguments})"
        if target == "rust" and kind == "list":
            list_type = parse_type(value["type"])
            item_type = list_type.arguments[0].canonical()
            items = ", ".join(
                self.coerce(self.expr(item, env), self.expression_type(item, env), item_type)
                for item in value["items"]
            )
            return f"vec![{items}]"
        if target == "rust" and kind == "record":
            record_name = value["type"]
            fields_by_name = {field["name"]: field["value"] for field in value["fields"]}
            fields = ", ".join(
                f"{name}: {self.coerce(self.expr(fields_by_name[name], env), self.expression_type(fields_by_name[name], env), field_type)}"
                for name, field_type in self.records[record_name].items()
            )
            return f"{record_name} {{ {fields} }}"
        if target == "rust" and kind == "member":
            return f"{self.expr(value['value'], env)}.{value['name']}"
        if target == "rust" and kind == "index":
            rendered = f"{self.expr(value['value'], env)}[({self.expr(value['index'], env)}) as usize]"
            if value["type"] not in {"Bool", "Float", "Int"}:
                rendered += ".clone()"
            return rendered
        if target == "java" and kind == "list":
            list_type = parse_type(value["type"])
            item_type = list_type.arguments[0].canonical()
            items = ", ".join(
                self.coerce(self.expr(item, env), self.expression_type(item, env), item_type)
                for item in value["items"]
            )
            return f"List.of({items})"
        if target == "java" and kind == "record":
            record_name = value["type"]
            fields_by_name = {field["name"]: field["value"] for field in value["fields"]}
            fields = ", ".join(
                self.coerce(self.expr(fields_by_name[name], env), self.expression_type(fields_by_name[name], env), field_type)
                for name, field_type in self.records[record_name].items()
            )
            return f"new {record_name}({fields})"
        if target == "java" and kind == "member":
            return f"{self.expr(value['value'], env)}.{value['name']}()"
        if target == "java" and kind == "index":
            return f"{self.expr(value['value'], env)}.get((int) {self.expr(value['index'], env)})"
        if target == "csharp" and kind == "list":
            list_type = parse_type(value["type"])
            item_type = list_type.arguments[0].canonical()
            items = ", ".join(
                self.coerce(self.expr(item, env), self.expression_type(item, env), item_type)
                for item in value["items"]
            )
            return f"new {self.target_type(value['type'])} {{ {items} }}"
        if target == "csharp" and kind == "record":
            record_name = value["type"]
            fields_by_name = {field["name"]: field["value"] for field in value["fields"]}
            fields = ", ".join(
                self.coerce(self.expr(fields_by_name[name], env), self.expression_type(fields_by_name[name], env), field_type)
                for name, field_type in self.records[record_name].items()
            )
            return f"new {record_name}({fields})"
        if target == "csharp" and kind == "member":
            return f"{self.expr(value['value'], env)}.{value['name']}"
        if target == "csharp" and kind == "index":
            return f"{self.expr(value['value'], env)}[(int) {self.expr(value['index'], env)}]"
        if target == "swift" and kind == "list":
            list_type = parse_type(value["type"])
            item_type = list_type.arguments[0].canonical()
            items = ", ".join(
                self.coerce(self.expr(item, env), self.expression_type(item, env), item_type)
                for item in value["items"]
            )
            return f"[{items}]"
        if target == "swift" and kind == "record":
            record_name = value["type"]
            fields_by_name = {field["name"]: field["value"] for field in value["fields"]}
            fields = ", ".join(
                f"{name}: {self.coerce(self.expr(fields_by_name[name], env), self.expression_type(fields_by_name[name], env), field_type)}"
                for name, field_type in self.records[record_name].items()
            )
            return f"{record_name}({fields})"
        if target == "swift" and kind == "member":
            return f"{self.expr(value['value'], env)}.{value['name']}"
        if target == "swift" and kind == "index":
            return f"{self.expr(value['value'], env)}[Int({self.expr(value['index'], env)})]"
        if target == "c" and kind == "list":
            list_type = parse_type(value["type"])
            item_type = list_type.arguments[0].canonical()
            rendered = [
                self.coerce(self.expr(item, env), self.expression_type(item, env), item_type)
                for item in value["items"]
            ]
            alias = self.target_type(value["type"])
            if not rendered:
                return f"({alias}){{ .items = NULL, .len = 0 }}"
            items = ", ".join(rendered)
            return f"aml_make_{alias}(({self.target_type(item_type)}[]){{{items}}}, {len(rendered)})"
        if target == "c" and kind == "record":
            record_name = value["type"]
            fields_by_name = {field["name"]: field["value"] for field in value["fields"]}
            fields = ", ".join(
                f".{name} = {self.coerce(self.expr(fields_by_name[name], env), self.expression_type(fields_by_name[name], env), field_type)}"
                for name, field_type in self.records[record_name].items()
            )
            return f"({record_name}){{ {fields} }}"
        if target == "c" and kind == "member":
            return f"({self.expr(value['value'], env)}).{value['name']}"
        if target == "c" and kind == "index":
            return f"({self.expr(value['value'], env)}).items[(size_t) ({self.expr(value['index'], env)})]"
        raise CompileError(f"{target} scalar-core backend does not support {kind} expressions", 1)

    def coerce(self, expression: str, actual: str, expected: str) -> str:
        if self.target.name == "rust":
            expected_ref = parse_type(expected)
            if expected_ref.name == "Optional":
                inner = expected_ref.arguments[0].canonical()
                if actual == "None":
                    return "None"
                if actual == inner:
                    return f"Some({expression})"
        if self.target.name == "java":
            expected_ref = parse_type(expected)
            if expected_ref.name == "Optional":
                inner = expected_ref.arguments[0].canonical()
                if actual == "None":
                    return "Optional.empty()"
                if actual == inner:
                    return f"Optional.of({expression})"
        if self.target.name == "csharp":
            expected_ref = parse_type(expected)
            if expected_ref.name == "Optional":
                return "null" if actual == "None" else expression
        if self.target.name == "swift":
            expected_ref = parse_type(expected)
            if expected_ref.name == "Optional":
                return "nil" if actual == "None" else expression
        if self.target.name == "c":
            expected_ref = parse_type(expected)
            if expected_ref.name == "Optional":
                alias = self.target_type(expected)
                if actual == "None":
                    return f"({alias}){{ .has = false }}"
                return f"({alias}){{ .has = true, .value = {expression} }}"
        if actual == expected or expected != "Float" or actual != "Int":
            return expression
        if self.target.name == "rust": return f"({expression} as f64)"
        if self.target.name == "swift": return f"Double({expression})"
        return expression

    def default(self, aml_type: str | None) -> str:
        if aml_type is None: return ""
        target = self.target.name
        if aml_type == "Bool": return self.target.false
        if aml_type == "Float": return "0.0"
        if aml_type == "Int": return "Int64(0)" if target == "swift" else "0"
        if aml_type == "Text": return "String::new()" if target == "rust" else "String()" if target == "swift" else '""'
        return ""

    def failure(self, message: str) -> str:
        target = self.target.name
        if target == "c": return f"aml_fail({message});"
        if target == "rust": return f"panic!(\"AML_ERROR: {{}}\", {message});"
        if target == "java": return f"throw new RuntimeException({message});"
        if target == "csharp": return f"throw new Exception({message});"
        return f"fatalError({message})"

    def condition_failure(self, condition: str, task: str, kind: str) -> str:
        message = self.string(f"contract | {task} {kind} failed")
        return f"if (!({condition})) {{ {self.failure(message)} }}"

    def show(self, expression: str, aml_type: str) -> str:
        target = self.target.name
        if target == "c":
            if aml_type == "Text": return f'printf("%s\\n", {expression});'
            if aml_type == "Int": return f'printf("%lld\\n", {expression});'
            if aml_type == "Float": return f'printf("%g\\n", {expression});'
            return f'printf("%s\\n", ({expression}) ? "true" : "false");'
        if target == "rust":
            if aml_type in SCALARS:
                return f"println!(\"{{}}\", {expression});"
            return f"println!(\"{{:?}}\", {expression});"
        if target == "java": return f"System.out.println({expression});"
        if target == "csharp": return f"Console.WriteLine({expression});"
        return f"print({expression})"

    def statement_end(self) -> str:
        return "" if self.target.name == "swift" else ";"

    def body(
        self,
        statements: list[dict[str, Any]],
        task: dict[str, Any],
        env: dict[str, str],
        level: int,
        ensures: list[dict[str, Any]] | None = None,
    ) -> list[str]:
        target = self.target.name
        indent = self.target.indent * level
        output: list[str] = []
        if ensures is None:
            ensures = [item["expression"] for item in statements if item["kind"] == "ensure"]
        for statement in statements:
            kind = statement["kind"]
            if kind == "ensure": continue
            if kind == "let":
                aml_type = self.expression_type(statement["value"], env)
                env[statement["name"]] = aml_type
                expression = self.expr(statement["value"], env)
                if target == "c": line = f"{self.target_type(aml_type)} {statement['name']} = {expression};"
                elif target == "rust": line = f"let {statement['name']} = {expression};"
                elif target == "java": line = f"{self.target_type(aml_type)} {statement['name']} = {expression};"
                elif target == "csharp": line = f"var {statement['name']} = {expression};"
                else: line = f"let {statement['name']} = {expression}"
                output.append(indent + line)
            elif kind == "show":
                aml_type = self.expression_type(statement["expression"], env)
                output.append(indent + self.show(self.expr(statement["expression"], env), aml_type))
            elif kind == "fail":
                output.append(indent + self.failure(self.expr(statement["expression"], env)))
            elif kind == "require":
                output.append(indent + self.condition_failure(self.expr(statement["expression"], env), task["name"], "require"))
            elif kind == "return":
                if statement["value"] is None:
                    output.append(indent + ("return" if target == "swift" else "return;"))
                    continue
                expression = self.expr(statement["value"], env)
                if task["return"]:
                    expression = self.coerce(
                        expression,
                        self.expression_type(statement["value"], env),
                        task["return"],
                    )
                if ensures:
                    result_type = task["return"] or self.expression_type(statement["value"], env)
                    declaration = {
                        "c": f"{self.target_type(result_type)} result = {expression};",
                        "rust": f"let result = {expression};",
                        "java": f"{self.target_type(result_type)} result = {expression};",
                        "csharp": f"var result = {expression};",
                        "swift": f"let result = {expression}",
                    }[target]
                    output.append(indent + declaration)
                    local = dict(env, result=result_type)
                    for condition in ensures:
                        output.append(indent + self.condition_failure(self.expr(condition, local), task["name"], "ensure"))
                    expression = "result"
                output.append(indent + f"return {expression}{'' if target == 'swift' else ';'}")
            elif kind == "if":
                condition = self.expr(statement["condition"], env)
                if target == "rust": header = f"if {condition} {{"
                elif target == "swift": header = f"if {condition} {{"
                else: header = f"if ({condition}) {{"
                output.append(indent + header)
                output.extend(self.body(statement["then"], task, dict(env), level + 1, ensures))
                if statement["else"]:
                    output.append(indent + "} else {")
                    output.extend(self.body(statement["else"], task, dict(env), level + 1, ensures))
                output.append(indent + "}")
            elif kind == "expression":
                output.append(indent + self.expr(statement["expression"], env) + self.statement_end())
        return output

    def signature(self, task: dict[str, Any]) -> str:
        target = self.target.name
        name = self.name(task["name"])
        result = self.target_type(task["return"]) if task["return"] else self.target.void
        if target == "c":
            params = ", ".join(f"{self.target_type(item['type'])} {item['name']}" for item in task["parameters"]) or "void"
            return f"{result} {name}({params})"
        if target == "rust":
            params = ", ".join(f"{item['name']}: {self.target_type(item['type'])}" for item in task["parameters"])
            suffix = f" -> {result}" if task["return"] else ""
            return f"fn {name}({params}){suffix}"
        if target == "java":
            params = ", ".join(f"{self.target_type(item['type'])} {item['name']}" for item in task["parameters"])
            return f"static {result} {name}({params})"
        if target == "csharp":
            params = ", ".join(f"{self.target_type(item['type'])} {item['name']}" for item in task["parameters"])
            return f"static {result} {name}({params})"
        params = ", ".join(f"_ {item['name']}: {self.target_type(item['type'])}" for item in task["parameters"])
        suffix = f" -> {result}" if task["return"] else ""
        return f"func {name}({params}){suffix}"

    def task(self, task: dict[str, Any], level: int = 0) -> list[str]:
        indent = self.target.indent * level
        env = {item["name"]: item["type"] for item in task["parameters"]}
        output = [indent + self.signature(task) + " {"]
        output.extend(self.body(task["body"], task, env, level + 1))
        if task["return"] and not self.definitely_returns(task["body"]):
            output.append(self.target.indent * (level + 1) + f"return {self.default(task['return'])}{'' if self.target.name == 'swift' else ';'}")
        output.append(indent + "}")
        return output

    def definitely_returns(self, statements: list[dict[str, Any]]) -> bool:
        for statement in statements:
            if statement["kind"] == "return":
                return True
            if statement["kind"] == "if" and statement["else"]:
                if self.definitely_returns(statement["then"]) and self.definitely_returns(statement["else"]):
                    return True
        return False

    def c_definitions(self) -> list[str]:
        output = [f"typedef struct {name} {name};" for name in self.records]
        emitted: set[str] = set(SCALARS)
        active: set[str] = set()

        def emit(aml_type: str) -> None:
            if aml_type in emitted:
                return
            if aml_type in active:
                raise CompileError(f"C backend does not yet support recursive type {aml_type}", 1)
            active.add(aml_type)
            parsed = parse_type(aml_type)
            if parsed.name == "List":
                inner = parsed.arguments[0].canonical()
                if parse_type(inner).name in {"List", "Optional"}:
                    emit(inner)
                alias = self.target_type(aml_type)
                item_type = self.target_type(inner)
                output.extend(
                    [
                        f"typedef struct {{ {item_type} *items; size_t len; }} {alias};",
                        f"static {alias} aml_make_{alias}({item_type} *items, size_t len) {{",
                        f"    {alias} result = {{ .items = NULL, .len = len }};",
                        "    if (len) {",
                        f"        result.items = malloc(sizeof({item_type}) * len);",
                        '        if (!result.items) { fprintf(stderr, "AML_ERROR: out of memory\\n"); exit(1); }',
                        f"        memcpy(result.items, items, sizeof({item_type}) * len);",
                        "    }",
                        "    return result;",
                        "}",
                    ]
                )
            elif parsed.name == "Optional":
                inner = parsed.arguments[0].canonical()
                emit(inner)
                alias = self.target_type(aml_type)
                output.append(f"typedef struct {{ bool has; {self.target_type(inner)} value; }} {alias};")
            elif parsed.name in self.records:
                for field_type in self.records[parsed.name].values():
                    emit(field_type)
                output.append(f"struct {parsed.name} {{")
                for field, field_type in self.records[parsed.name].items():
                    output.append(f"    {self.target_type(field_type)} {field};")
                output.append("};")
            else:
                raise CompileError(f"C backend cannot define type {aml_type}", 1)
            active.remove(aml_type)
            emitted.add(aml_type)

        for record_name in self.records:
            emit(record_name)
        for parameters, return_type in self.signatures.values():
            for aml_type in [*parameters, *([return_type] if return_type else [])]:
                emit(aml_type)
        return output

    def render(self) -> str:
        self.validate()
        target = self.target.name
        tasks: list[str] = []
        level = 1 if target in {"java", "csharp"} else 0
        for task in self.ir["tasks"]:
            tasks.extend(self.task(task, level) + [""])
        has_main = "main" in self.signatures
        if target == "c":
            prototypes = [self.signature(task) + ";" for task in self.ir["tasks"]]
            entry = ["int main(void) {", "    aml_main();", "    return 0;", "}"] if has_main else []
            definitions = self.c_definitions()
            return "\n".join([
                "/* Generated by AML 0.8.0.dev0. */", "#include <stdbool.h>", "#include <stdio.h>", "#include <stdlib.h>", "#include <string.h>", "",
                "static void aml_fail(const char *message) { fprintf(stderr, \"AML_ERROR: %s\\n\", message); exit(1); }",
                "static char *aml_text_add(const char *a, const char *b) { size_t n = strlen(a) + strlen(b) + 1; char *r = malloc(n); if (!r) aml_fail(\"out of memory\"); strcpy(r, a); strcat(r, b); return r; }",
                "", *definitions, "", *prototypes, "", *tasks, *entry, "",
            ])
        if target == "rust":
            entry = ["fn main() { aml_main(); }"] if has_main else []
            records: list[str] = []
            for record in self.ir["records"]:
                records.extend(["#[derive(Clone, Debug, PartialEq)]", f"struct {record['name']} {{"])
                for field in record["fields"]:
                    records.append(f"    {field['name']}: {self.target_type(field['type'])},")
                records.extend(["}", ""])
            return "\n".join(["// Generated by AML 0.8.0.dev0.", "", *records, *tasks, *entry, ""])
        if target == "java":
            entry = ["    public static void main(String[] args) { aml_main(); }"] if has_main else []
            records: list[str] = []
            for record in self.ir["records"]:
                fields = ", ".join(
                    f"{self.target_type(field['type'])} {field['name']}" for field in record["fields"]
                )
                records.extend([f"    record {record['name']}({fields}) {{}}", ""])
            return "\n".join(["// Generated by AML 0.8.0.dev0.", "import java.util.*;", "", "class AMLProgram {", *records, *tasks, *entry, "}", ""])
        if target == "csharp":
            entry = ["    public static void Main(string[] args) { aml_main(); }"] if has_main else []
            records: list[str] = []
            for record in self.ir["records"]:
                fields = ", ".join(
                    f"{self.target_type(field['type'])} {field['name']}" for field in record["fields"]
                )
                records.extend([f"    record {record['name']}({fields});", ""])
            return "\n".join(["// Generated by AML 0.8.0.dev0.", "using System;", "using System.Collections.Generic;", "", "class AMLProgram {", *records, *tasks, *entry, "}", ""])
        entry = ["aml_main()"] if has_main else []
        records: list[str] = []
        for record in self.ir["records"]:
            records.extend([f"struct {record['name']}: Equatable {{"])
            for field in record["fields"]:
                records.append(f"    let {field['name']}: {self.target_type(field['type'])}")
            records.extend(["}", ""])
        return "\n".join(["// Generated by AML 0.8.0.dev0.", "", *records, *tasks, *entry, ""])


def compile_static(source: str, filename: str, target: str) -> str:
    if target not in TARGETS:
        raise ValueError(f"unknown static target: {target}")
    return Renderer(TARGETS[target], program_ir(source, filename)).render()
