# SPDX-License-Identifier: MIT OR Apache-2.0
# gdlint: disable=max-returns
# Fail-fast parsing keeps the first diagnostic and avoids deeply nested branches.
@tool
extends RefCounted

const MAX_BYTES := 8 * 1024 * 1024
const MAX_DEPTH := 64
const MAX_VALUES := 500_000
const MAX_STRING_BYTES := 1024 * 1024
const MAX_NUMBER_BYTES := 128
const WS := [0x20, 0x09, 0x0a, 0x0d]

var _text := ""
var _at := 0
var _values := 0
var _error: Dictionary = {}


func decode(bytes: PackedByteArray) -> Dictionary:
	_at = 0
	_values = 0
	_error = {}
	if bytes.size() > MAX_BYTES:
		return _failure("input_limit")
	if not _valid_utf8(bytes):
		return _failure("invalid_utf8")
	_text = bytes.get_string_from_utf8()
	if _text.begins_with(String.chr(0xfeff)):
		_text = _text.substr(1)
	var value: Variant = _value(0)
	_space()
	if _error.is_empty() and _at != _text.length():
		_fail("invalid_json")
	if not _error.is_empty():
		return {"ok": false, "error": _error}
	return {"ok": true, "data": value}


func _failure(code: String) -> Dictionary:
	return {"ok": false, "error": {"code": code, "path": "$", "offset": _at}}


func _fail(code: String) -> void:
	if _error.is_empty():
		_error = {"code": code, "path": "$", "offset": _at}


func _valid_utf8(bytes: PackedByteArray) -> bool:
	var i := 0
	while i < bytes.size():
		var first := bytes[i]
		if first == 0:
			return false
		if first < 0x80:
			i += 1
			continue
		var width := 0
		var minimum := 0
		var point := 0
		if first >= 0xc2 and first <= 0xdf:
			width = 2
			minimum = 0x80
			point = first & 0x1f
		elif first >= 0xe0 and first <= 0xef:
			width = 3
			minimum = 0x800
			point = first & 0x0f
		elif first >= 0xf0 and first <= 0xf4:
			width = 4
			minimum = 0x10000
			point = first & 0x07
		else:
			return false
		if i + width > bytes.size():
			return false
		for j in range(1, width):
			var next := bytes[i + j]
			if next < 0x80 or next > 0xbf:
				return false
			point = (point << 6) | (next & 0x3f)
		if point < minimum or point > 0x10ffff or (point >= 0xd800 and point <= 0xdfff):
			return false
		i += width
	return true


func _space() -> void:
	while _at < _text.length() and _text.unicode_at(_at) in WS:
		_at += 1


func _value(depth: int) -> Variant:
	if not _error.is_empty():
		return null
	if depth > MAX_DEPTH:
		_fail("depth_limit")
		return null
	_values += 1
	if _values > MAX_VALUES:
		_fail("value_limit")
		return null
	_space()
	if _at >= _text.length():
		_fail("invalid_json")
		return null
	var token := _text.unicode_at(_at)
	match token:
		0x7b:
			return _object(depth)
		0x5b:
			return _array(depth)
		0x22:
			return _string()
		0x74:
			return _literal("true", true)
		0x66:
			return _literal("false", false)
		0x6e:
			return _literal("null", null)
		_:
			if token == 0x2d or (token >= 0x30 and token <= 0x39):
				return _number()
			_fail("invalid_json")
			return null


func _literal(word: String, value: Variant) -> Variant:
	if _text.substr(_at, word.length()) != word:
		_fail("invalid_json")
		return null
	_at += word.length()
	return value


func _object(depth: int) -> Dictionary:
	_at += 1
	_space()
	var result: Dictionary = {}
	if _take("}"):
		return result
	while _error.is_empty():
		if not _peek('"'):
			_fail("invalid_json")
			break
		var key := _string()
		if result.has(key):
			_fail("duplicate_key")
			break
		_space()
		if not _take(":"):
			_fail("invalid_json")
			break
		result[key] = _value(depth + 1)
		_space()
		if _take("}"):
			return result
		if not _take(","):
			_fail("invalid_json")
			break
		_space()
	return {}


func _array(depth: int) -> Array:
	_at += 1
	_space()
	var result: Array = []
	if _take("]"):
		return result
	while _error.is_empty():
		result.append(_value(depth + 1))
		_space()
		if _take("]"):
			return result
		if not _take(","):
			_fail("invalid_json")
			break
	return []


func _peek(token: String) -> bool:
	return _at < _text.length() and _text.substr(_at, 1) == token


func _take(token: String) -> bool:
	if _peek(token):
		_at += 1
		return true
	return false


func _string() -> String:
	_at += 1
	var parts := PackedStringArray()
	var byte_count := 0
	while _at < _text.length() and _error.is_empty():
		var point := _text.unicode_at(_at)
		_at += 1
		if point == 0x22:
			return "".join(parts)
		if point < 0x20:
			_fail("invalid_json_string")
			break
		if point == 0x5c:
			if _at >= _text.length():
				_fail("invalid_json_string")
				break
			var escape := _text.substr(_at, 1)
			_at += 1
			var escapes := {
				'"': 0x22, "\\": 0x5c, "/": 0x2f, "b": 8, "f": 12, "n": 10, "r": 13, "t": 9
			}
			if escapes.has(escape):
				point = escapes[escape]
			elif escape == "u":
				point = _hex4()
				if point >= 0xd800 and point <= 0xdbff:
					if _text.substr(_at, 2) != "\\u":
						_fail("invalid_unicode")
						break
					_at += 2
					var low := _hex4()
					if low < 0xdc00 or low > 0xdfff:
						_fail("invalid_unicode")
						break
					point = 0x10000 + ((point - 0xd800) << 10) + low - 0xdc00
				elif point >= 0xdc00 and point <= 0xdfff:
					_fail("invalid_unicode")
					break
			else:
				_fail("invalid_json_string")
				break
		if not _error.is_empty():
			break
		if point == 0:
			_fail("unrepresentable_nul")
			break
		byte_count += 1 if point < 0x80 else (2 if point < 0x800 else (3 if point < 0x10000 else 4))
		if byte_count > MAX_STRING_BYTES:
			_fail("string_limit")
			break
		parts.append(String.chr(point))
	_fail("invalid_json_string")
	return ""


func _hex4() -> int:
	if _at + 4 > _text.length():
		_fail("invalid_unicode")
		return -1
	var result := 0
	for i in range(4):
		var point := _text.unicode_at(_at + i)
		var digit := -1
		if point >= 0x30 and point <= 0x39:
			digit = point - 0x30
		elif point >= 0x61 and point <= 0x66:
			digit = point - 0x61 + 10
		elif point >= 0x41 and point <= 0x46:
			digit = point - 0x41 + 10
		if digit < 0:
			_fail("invalid_unicode")
			return -1
		result = (result << 4) | digit
	_at += 4
	return result


func _digits() -> int:
	var start := _at
	while _at < _text.length() and _text.unicode_at(_at) >= 0x30 and _text.unicode_at(_at) <= 0x39:
		_at += 1
	return _at - start


func _number() -> Variant:
	var start := _at
	var negative := _take("-")
	if _take("0"):
		if _at < _text.length() and _text.unicode_at(_at) >= 0x30 and _text.unicode_at(_at) <= 0x39:
			_fail("invalid_number")
	elif _digits() == 0:
		_fail("invalid_number")
	var floating := false
	if _take("."):
		floating = true
		if _digits() == 0:
			_fail("invalid_number")
	if _take("e") or _take("E"):
		floating = true
		if not _take("+"):
			_take("-")
		if _digits() == 0:
			_fail("invalid_number")
	var number := _text.substr(start, _at - start)
	if not _error.is_empty():
		return null
	if floating:
		# PG-03 serializes finite IEEE-754 values with short decimal tokens.
		if number.length() > MAX_NUMBER_BYTES:
			_fail("number_token_limit")
			return null
		var exponent := number.to_lower().find("e")
		if exponent >= 0:
			if not _exponent_range(number.substr(exponent + 1)):
				return null
		var result := number.to_float()
		if not is_finite(result):
			_fail("number_range")
		return result
	var digits := number.substr(1) if negative else number
	var maximum := "9223372036854775808" if negative else "9223372036854775807"
	if (
		digits.length() > maximum.length()
		or (digits.length() == maximum.length() and digits > maximum)
	):
		_fail("number_range")
		return null
	return number.to_int()


func _exponent_range(exponent: String) -> bool:
	var offset := 1 if exponent.begins_with("-") or exponent.begins_with("+") else 0
	var maximum := 400 if exponent.begins_with("-") else 308
	var absolute := 0
	for i in range(offset, exponent.length()):
		absolute = absolute * 10 + exponent.unicode_at(i) - 0x30
		if absolute > maximum:
			_fail("number_range")
			return false
	return true
