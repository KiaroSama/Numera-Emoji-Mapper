"""Version-independent source behavior fingerprint for relocated test references."""
import ast
import hashlib
import json


class RelocationOnly(ast.NodeTransformer):
    def visit_Import(self, node):
        return None

    def visit_ImportFrom(self, node):
        return None

    def visit_Assign(self, node):
        if any(isinstance(target, ast.Name) and target.id in {"ROOT", "OFFSET_FILE"}
               for target in node.targets):
            return None
        return self.generic_visit(node)


def digest(text: str) -> str:
    def stable(value):
        if isinstance(value, ast.AST):
            return [type(value).__name__, [[name, stable(getattr(value, name, None))]
                    for name in value._fields if name != "type_params" or getattr(value, name, None)]]
        if isinstance(value, list):
            return [stable(item) for item in value]
        if isinstance(value, bytes):
            return {"bytes": value.hex()}
        if isinstance(value, complex):
            return {"complex": [value.real, value.imag]}
        if value is Ellipsis:
            return {"ellipsis": True}
        return value
    tree = RelocationOnly().visit(ast.parse(text))
    encoded = json.dumps(stable(tree), ensure_ascii=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()
