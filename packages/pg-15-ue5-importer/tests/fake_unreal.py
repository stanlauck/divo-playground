# SPDX-License-Identifier: MIT OR Apache-2.0
"""Small fake Unreal API used by offline tests."""
from types import SimpleNamespace


class Vector:
    def __init__(self, x=0, y=0, z=0): self.x, self.y, self.z = x, y, z


class Rotator:
    def __init__(self, *args): self.args = args


class Actor:
    def __init__(self, location): self.location, self.label, self.tags = location, '', []
    def get_editor_property(self, name): return getattr(self, name)
    def set_editor_property(self, name, value): setattr(self, name, value)
    def get_actor_location(self): return self.location
    def set_actor_location(self, value, *_): self.location = value; return True
    def set_actor_label(self, value, *_): self.label = value
    def get_actor_label(self): return self.label


class EditorActorSubsystem:
    actors = []
    def get_all_level_actors(self): return list(self.actors)
    def spawn_actor_from_class(self, klass, location, _rotation):
        actor = klass(location); self.actors.append(actor); return actor
    def destroy_actor(self, actor):
        if actor in self.actors: self.actors.remove(actor); return True
        return False


class Paths:
    directory = '/tmp/storyworld-fake-saved'
    @classmethod
    def project_saved_dir(cls): return cls.directory


def module():
    subsystem = EditorActorSubsystem()
    return SimpleNamespace(Vector=Vector, Rotator=Rotator, Actor=Actor, EditorActorSubsystem=EditorActorSubsystem, Paths=Paths, get_editor_subsystem=lambda _: subsystem)
