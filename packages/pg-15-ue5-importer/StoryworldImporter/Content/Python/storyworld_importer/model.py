# SPDX-License-Identifier: MIT OR Apache-2.0
"""Bounded JSON parsing, local schema validation and graph integrity checks."""
from __future__ import annotations

import json
import math
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

MAX_INPUT_BYTES = 8 * 1024 * 1024
MAX_DEPTH = 64
MAX_VALUES = 500_000
MAX_STRING_BYTES = 1024 * 1024
SCHEMA = json.loads(Path(__file__).with_name('schema.json').read_text(encoding='utf-8'))


class StoryworldImportError(ValueError):
    """A validation failure with a structural JSON path, never a source filename."""
    def __init__(self, code: str, path: str, message: str):
        self.code, self.path, self.message = code, path, message
        super().__init__(f'{code} at {path}: {message}')

    def to_dict(self) -> dict[str, str]:
        return dict(code=self.code, path=self.path, message=self.message)


@dataclass(frozen=True)
class StoryworldDocument:
    data: dict[str, Any]
    warnings: tuple[str, ...] = field(default_factory=tuple)

    @property
    def world(self):
        return self.data['world']

    @property
    def dialogue(self):
        return self.data['dialogue']

    @property
    def records(self):
        for kind, plural in [('location', 'locations'), ('exit', 'exits'), ('item', 'items')]:
            for record in self.world[plural]:
                yield kind, record


def require(ok, code, path, message):
    if not ok:
        raise StoryworldImportError(code, path, message)


class Pairs(list):
    """Retain decoded object pairs until paths can be assigned."""


def materialize(value, path='$', depth=0, budget=None):
    budget = budget if budget is not None else [0]
    budget[0] += 1
    require(depth <= MAX_DEPTH, 'depth_limit', path, 'JSON nesting exceeds 64')
    require(budget[0] <= MAX_VALUES, 'value_limit', path, 'too many JSON values')
    if isinstance(value, Pairs):
        result = {}
        for key, child in value:
            materialize(key, path, depth, budget)
            child_path = f'{path}.{key}'
            require(key not in result, 'duplicate_key', child_path, 'duplicate decoded key')
            result[key] = materialize(child, child_path, depth + 1, budget)
        return result
    if isinstance(value, list):
        return [materialize(v, f'{path}[{i}]', depth + 1, budget) for i, v in enumerate(value)]
    if isinstance(value, str):
        require(not any(0xD800 <= ord(c) <= 0xDFFF or ord(c) == 0 for c in value), 'string', path, 'NUL and lone surrogates are unsupported')
        require(len(value.encode('utf-8')) <= MAX_STRING_BYTES, 'string_limit', path, 'string exceeds 1 MiB')
    if type(value) is int:
        require(-(2**63) <= value < 2**63, 'number', path, 'integer outside signed int64')
    if type(value) is float:
        require(math.isfinite(value), 'number', path, 'nonfinite number')
    return value


def schema_validate(value, schema, path='$'):
    """Implement only the schema keywords present in the bundled v1 schema."""
    if '$ref' in schema:
        return schema_validate(value, SCHEMA['$defs'][schema['$ref'].split('/')[-1]], path)
    for keyword in ('oneOf', 'anyOf'):
        if keyword in schema:
            successes = 0
            for alternative in schema[keyword]:
                try:
                    schema_validate(value, alternative, path)
                    successes += 1
                except StoryworldImportError:
                    pass
            require(successes == 1 if keyword == 'oneOf' else successes > 0, 'shape', path, 'no matching JSON shape')
    if 'not' in schema:
        try:
            schema_validate(value, schema['not'], path)
        except StoryworldImportError:
            pass
        else:
            require(False, 'shape', path, 'forbidden value')
    for child in schema.get('allOf', []):
        if 'if' in child:
            try:
                schema_validate(value, child['if'], path)
            except StoryworldImportError:
                continue
            schema_validate(value, child['then'], path)
        else:
            schema_validate(value, child, path)
    if 'const' in schema:
        require(type(value) is type(schema['const']) and value == schema['const'], 'version' if path.endswith('.version') else 'value', path, 'unexpected constant')
    if 'enum' in schema:
        require(value in schema['enum'], 'enum', path, 'unknown enumeration value')
    types = {'object': lambda: isinstance(value, dict), 'array': lambda: isinstance(value, list), 'integer': lambda: type(value) is int, 'number': lambda: type(value) in (int, float), 'string': lambda: isinstance(value, str), 'boolean': lambda: type(value) is bool, 'null': lambda: value is None}
    if 'type' in schema:
        require(types[schema['type']](), 'type', path, 'expected ' + schema['type'])
    if isinstance(value, dict):
        missing = sorted(set(schema.get('required', [])) - set(value))
        if missing:
            require(False, 'missing_field', f'{path}.{missing[0]}', 'required field')
        properties = schema.get('properties', {})
        if schema.get('additionalProperties') is False:
            unknown = sorted(set(value) - set(properties))
            if unknown:
                require(False, 'unknown_field', f'{path}.{unknown[0]}', 'unknown structural field')
        for key in properties:
            if key in value:
                schema_validate(value[key], properties[key], f'{path}.{key}')
    if isinstance(value, list):
        require(len(value) <= schema.get('maxItems', MAX_VALUES), 'array_limit', path, 'array exceeds limit')
        require(len(value) >= schema.get('minItems', 0), 'array_limit', path, 'array too short')
        if schema.get('uniqueItems'):
            keys = [json.dumps(v, sort_keys=True) for v in value]
            require(len(keys) == len(set(keys)), 'duplicate_id', path, 'duplicate array element')
        for i, child in enumerate(value):
            schema_validate(child, schema.get('items', {}), f'{path}[{i}]')
    if isinstance(value, str):
        require(len(value) >= schema.get('minLength', 0), 'identifier', path, 'empty string')
        require(len(value.encode('utf-8')) <= schema.get('maxLength', MAX_STRING_BYTES), 'identifier', path, 'string too long')
        if 'pattern' in schema:
            require(re.search(schema['pattern'], value) is not None, 'identifier', path, 'invalid identifier')
    if type(value) in (int, float):
        require(value >= schema.get('minimum', -math.inf) and value <= schema.get('maximum', math.inf), 'number', path, 'number outside range')


def index_records(records, key, path):
    result = {}
    for i, record in enumerate(records):
        rid = record[key]
        require(rid not in result, 'duplicate_id', f'{path}[{i}].{key}', 'duplicate id')
        result[rid] = record
    return result


def validate_integrity(data):
    world, graph = data['world'], data['dialogue']
    warnings = []
    all_ids = set()
    locations = {r['id'] for r in world['locations']}
    nodes = index_records(graph['nodes'], 'id', '$.dialogue.nodes')
    pins = index_records(graph['pins'], 'id', '$.dialogue.pins')
    edges = index_records(graph['edges'], 'id', '$.dialogue.edges')
    index_records(graph['choices'], 'edge', '$.dialogue.choices')
    packages = index_records(graph['packages'], 'index', '$.dialogue.packages')
    namespaces = index_records(graph['variable_namespaces'], 'name', '$.dialogue.variable_namespaces')
    variables = {}
    for i, var in enumerate(graph['variables']):
        key = (var['namespace'], var['name'])
        require(key not in variables, 'duplicate_id', f'$.dialogue.variables[{i}]', 'duplicate variable')
        require(var['namespace'] in namespaces and var['name'] in namespaces[var['namespace']]['variables'], 'reference', f'$.dialogue.variables[{i}]', 'variable namespace membership mismatch')
        variables[key] = var
    for namespace in namespaces.values():
        for name in namespace['variables']:
            require((namespace['name'], name) in variables, 'reference', '$.dialogue.variable_namespaces', 'missing variable')

    def ref(rid, table, path, filtered=False):
        if rid is None or rid in table:
            return
        if filtered:
            warnings.append(f'{path}: missing filtered reference {rid}')
        else:
            require(False, 'dangling_reference', path, 'unknown reference')

    def expression(value, path, depth=0):
        require(depth <= 32, 'depth_limit', path, 'declarative nesting exceeds 32')
        op = value['op']
        if op in ('eq', 'ne', 'set'):
            key = (value['variable']['namespace'], value['variable']['name'])
            require(key in variables, 'dangling_reference', path + '.variable', 'unknown variable')
            kind, literal = variables[key]['kind'], value['value']
            valid = {'boolean': type(literal) is bool, 'integer': type(literal) is int, 'float': type(literal) is float or (type(literal) is int and abs(literal) <= 2**53 - 1), 'string': isinstance(literal, str)}
            require(valid.get(kind, False), 'literal_type', path + '.value', 'literal does not match variable kind')
        elif op in ('all', 'any'):
            for i, child in enumerate(value['conditions']):
                expression(child, f'{path}.conditions[{i}]', depth + 1)
        elif op == 'not':
            expression(value['condition'], path + '.condition', depth + 1)

    def declarations(record, path):
        for key in ('condition', 'event'):
            if key in record:
                expression(record[key], path + '.' + key)
        if record.get('script') is not None:
            warnings.append(f'{path}.script: opaque_script_not_executed')
            if 'condition' in record or 'event' in record:
                warnings.append(f'{path}: declarative fields take precedence over opaque script')

    for plural in ('locations', 'exits', 'items'):
        for i, record in enumerate(world[plural]):
            path = f'$.world.{plural}[{i}]'
            require(record['id'] not in all_ids, 'duplicate_id', path + '.id', 'world ids must be unique')
            all_ids.add(record['id'])
            if plural == 'exits':
                ref(record['from'], locations, path + '.from')
                ref(record['to'], locations, path + '.to')
            if plural == 'items':
                ref(record['location'], locations, path + '.location')
            ref(record.get('dialogue'), nodes, path + '.dialogue')
            declarations(record, path)
    for i, package in enumerate(graph['packages']):
        for rid in package['node_ids']:
            ref(rid, nodes, f'$.dialogue.packages[{i}].node_ids', True)
            if rid in nodes:
                require(nodes[rid]['package'] == package['index'], 'ownership', '$.dialogue.packages', 'node package mismatch')
    listed_pins = set()
    for i, node in enumerate(graph['nodes']):
        path = f'$.dialogue.nodes[{i}]'
        require(node['package'] in packages and node['id'] in packages[node['package']]['node_ids'], 'ownership', path + '.package', 'node package membership mismatch')
        ref(node['parent'], nodes, path + '.parent', True)
        ref(node['speaker'], nodes, path + '.speaker', True)
        ancestors, parent = {node['id']}, node['parent']
        while parent in nodes:
            require(parent not in ancestors, 'cycle', path + '.parent', 'parent cycle')
            ancestors.add(parent)
            require(len(ancestors) <= 65, 'depth_limit', path + '.parent', 'hierarchy too deep')
            parent = nodes[parent]['parent']
        for direction in ('input', 'output'):
            for j, pid in enumerate(node[direction + '_pins']):
                ref(pid, pins, path + '.' + direction + '_pins', True)
                if pid in pins:
                    pin = pins[pid]
                    require(pid not in listed_pins and pin['owner'] == node['id'] and pin['direction'] == direction and pin['index'] == j, 'ownership', path, 'pin ownership/direction/index mismatch')
                    listed_pins.add(pid)
        declarations(node, path)
    for i, pin in enumerate(graph['pins']):
        path = f'$.dialogue.pins[{i}]'
        ref(pin['owner'], nodes, path + '.owner')
        require(pin['id'] in listed_pins, 'ownership', path, 'pin not listed by owner')
        declarations(pin, path)
    for i, edge in enumerate(graph['edges']):
        path = f'$.dialogue.edges[{i}]'
        ref(edge['source'], nodes, path + '.source')
        ref(edge['target'], nodes, path + '.target', True)
        for side in ('source', 'target'):
            pid = edge[side + '_pin']
            ref(pid, pins, path + '.' + side + '_pin', True)
            if pid in pins:
                require(pins[pid]['owner'] == edge[side], 'ownership', path, 'edge pin owner mismatch')
    for i, choice in enumerate(graph['choices']):
        path = f'$.dialogue.choices[{i}]'
        ref(choice['edge'], edges, path + '.edge')
        edge = edges[choice['edge']]
        require(all(choice[k] == edge[k] for k in ('source', 'source_pin', 'target')), 'ownership', path, 'choice differs from edge')
        pid = choice['source_pin']
        ref(pid, pins, path + '.source_pin', True)
        if pid in pins:
            require(pins[pid]['direction'] == 'output', 'ownership', path, 'choice source must be output')
        declarations(choice, path)
    hierarchy = index_records(graph['hierarchy'], 'id', '$.dialogue.hierarchy')
    for i, entry in enumerate(graph['hierarchy']):
        path = f'$.dialogue.hierarchy[{i}]'
        ref(entry['id'], nodes, path + '.id', True)
        ref(entry['parent'], nodes, path + '.parent', True)
        seen, parent = {entry['id']}, entry['parent']
        while parent in hierarchy:
            require(parent not in seen, 'cycle', path, 'hierarchy cycle')
            seen.add(parent)
            parent = hierarchy[parent]['parent']
        if entry['id'] in nodes and entry['parent'] != nodes[entry['id']]['parent']:
            warnings.append(f'{path}: hierarchy_parent_mismatch')
    return tuple(sorted(warnings))


def load_storyworld(source: str | bytes | Path) -> StoryworldDocument:
    """Load a filename (str/Path) or JSON bytes; byte reads are capped."""
    if isinstance(source, bytes):
        raw = source
    else:
        try:
            with Path(source).open('rb') as stream:
                raw = stream.read(MAX_INPUT_BYTES + 1)
        except (OSError, ValueError) as exc:
            raise StoryworldImportError('io', '$', 'cannot read input file') from exc
    require(len(raw) <= MAX_INPUT_BYTES, 'input_limit', '$', 'input exceeds 8 MiB')
    try:
        text = raw.decode('utf-8-sig')
        parsed = json.loads(text, object_pairs_hook=Pairs, parse_constant=lambda token: (_ for _ in ()).throw(ValueError('nonfinite constant')))
    except UnicodeDecodeError as exc:
        raise StoryworldImportError('encoding', '$', 'invalid UTF-8') from exc
    except (ValueError, RecursionError) as exc:
        raise StoryworldImportError('json', '$', 'invalid JSON') from exc
    data = materialize(parsed)
    schema_validate(data, SCHEMA)
    warnings = validate_integrity(data)
    return StoryworldDocument(data, warnings)


load = load_storyworld
