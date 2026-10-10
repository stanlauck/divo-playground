# SPDX-License-Identifier: MIT OR Apache-2.0
"""The sole Unreal API boundary. Applies plans to the current editor level."""
from __future__ import annotations

import base64
import json
import re
from pathlib import Path

import unreal

from .plan import compute_plan
from .report import ImportReport


def tag_values(actor, prefix):
    return [str(t)[len(prefix):] for t in actor.get_editor_property('tags') if str(t).startswith(prefix)]


def subsystem():
    return unreal.get_editor_subsystem(unreal.EditorActorSubsystem)


def snapshot():
    values, actors = {}, {}
    for actor in subsystem().get_all_level_actors():
        ids = tag_values(actor, 'storyworld:id=')
        kinds = tag_values(actor, 'storyworld:kind=')
        if not ids and not kinds:
            continue
        if len(ids) != 1 or len(kinds) != 1 or kinds[0] not in ('location', 'exit', 'item'):
            raise ValueError('ambiguous storyworld identity tags')
        if ids[0] in actors:
            raise ValueError(f'duplicate storyworld id: {ids[0]}')
        location = actor.get_actor_location()
        values[ids[0]] = dict(kind=kinds[0], data=tag_values(actor, 'storyworld:data='), label=actor.get_actor_label(), transform=(location.x, location.y, location.z))
        actors[ids[0]] = actor
    return values, actors


def record_tag(record):
    encoded = json.dumps(record, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode('utf-8')
    return base64.b64encode(encoded).decode('ascii')


def update_actor(actor, spec, dialogue_path, report):
    actor.set_actor_label(spec.label, False)
    result = actor.set_actor_location(unreal.Vector(*spec.transform), False, True)
    if result is False:
        raise RuntimeError('actor location update failed')
    tags = [t for t in actor.get_editor_property('tags') if not str(t).startswith('storyworld:')]
    tags += [f'storyworld:id={spec.id}', f'storyworld:kind={spec.kind}', f'storyworld:data={record_tag(spec.metadata)}', f'storyworld:dialogue={dialogue_path}']
    actor.set_editor_property('tags', tags)
    # TextRenderActor supplies a correctly registered TextRenderComponent. Plain
    # Actor (the default) has no component and remains an Outliner placeholder.
    if hasattr(unreal, 'TextRenderComponent'):
        component = actor.get_component_by_class(unreal.TextRenderComponent)
        if component:
            component.set_text(spec.metadata.get('name', spec.id))


def apply_document(document, *, delete_orphans=False, dry_run=False, placeholder_class=None, name='storyworld'):
    report = ImportReport(warnings=list(document.warnings))
    try:
        current, actors = snapshot()
    except Exception as exc:
        report.errors.append(dict(code='identity', path='$', message=str(exc)))
        return report
    plan = compute_plan(document, current)
    report.orphans = [rid for _, rid in plan.orphans]
    if dry_run:
        report.created = [s.id for s in plan.create]
        report.updated = [s.id for s in plan.update]
        report.unchanged = [s.id for s in plan.unchanged]
        return report
    # The name is a filename stem, never an asset path or source metadata.
    safe_name = re.sub(r'[^A-Za-z0-9_.-]', '_', name).strip('.') or 'storyworld'
    target = Path(unreal.Paths.project_saved_dir()) / 'Storyworld' / f'{safe_name}.dialogue.json'
    try:
        target.parent.mkdir(parents=True, exist_ok=True)
        payload = json.dumps(document.dialogue, ensure_ascii=False, sort_keys=True, indent=2) + '\n'
        if not target.exists() or target.read_text(encoding='utf-8') != payload:
            temporary = target.with_suffix('.json.tmp')
            temporary.write_text(payload, encoding='utf-8')
            temporary.replace(target)
    except OSError:
        report.errors.append(dict(code='dialogue_io', path='$.dialogue', message='cannot save dialogue graph'))
        return report
    for action, specs in [('created', plan.create), ('updated', plan.update)]:
        for spec in specs:
            actor = None
            try:
                actor = subsystem().spawn_actor_from_class(placeholder_class or unreal.Actor, unreal.Vector(*spec.transform), unreal.Rotator()) if action == 'created' else actors[spec.id]
                if actor is None:
                    raise RuntimeError('actor spawn failed')
                update_actor(actor, spec, str(target), report)
                getattr(report, action).append(spec.id)
            except Exception as exc:
                if actor is not None and action == 'created':
                    subsystem().destroy_actor(actor)
                report.errors.append(dict(id=spec.id, code='apply', path='$', message=str(exc)))
    for spec in plan.unchanged:
        actor = actors[spec.id]
        # A renamed input may move the dialogue file without changing records.
        if tag_values(actor, 'storyworld:dialogue=') != [str(target)]:
            tags = [t for t in actor.get_editor_property('tags') if not str(t).startswith('storyworld:dialogue=')]
            actor.set_editor_property('tags', tags + [f'storyworld:dialogue={target}'])
        report.unchanged.append(spec.id)
    if delete_orphans:
        for _, rid in plan.orphans:
            if not subsystem().destroy_actor(actors[rid]):
                report.errors.append(dict(id=rid, code='delete', path='$', message='orphan deletion failed'))
    return report
