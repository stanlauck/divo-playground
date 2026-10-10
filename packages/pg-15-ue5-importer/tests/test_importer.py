# SPDX-License-Identifier: MIT OR Apache-2.0
import base64, copy, json, sys, tempfile, unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / 'StoryworldImporter' / 'Content' / 'Python'))
from tests.fake_unreal import module as fake_module
sys.modules['unreal'] = fake_module()
from storyworld_importer.model import StoryworldImportError, load_storyworld
from storyworld_importer.plan import compute_plan
from storyworld_importer.report import ImportReport
from storyworld_importer import import_storyworld
from storyworld_importer.unreal_apply import apply_document

ROOT = Path(__file__).parents[1]
SAMPLE = ROOT / 'samples' / 'declarative.storyworld'


def raw(): return json.loads(SAMPLE.read_text(encoding='utf8'))


class LoaderTests(unittest.TestCase):
    def test_samples(self):
        self.assertEqual(len(list(load_storyworld(SAMPLE).records)), 4)
        self.assertEqual(len(list(load_storyworld(ROOT / 'samples' / 'pg03-compatible.storyworld').records)), 4)
    def test_bom(self): self.assertEqual(load_storyworld(b'\xef\xbb\xbf' + SAMPLE.read_bytes()).data['version'], 1)
    def test_invalid_json(self):
        with self.assertRaises(StoryworldImportError): load_storyworld(b'{')
    def test_version_float(self):
        d=raw(); d['version']=1.0
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_dialogue_version_float(self):
        d=raw(); d['dialogue']['version']=1.0
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_unknown_root(self):
        d=raw(); d['extra']=1
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_unknown_record(self):
        d=raw(); d['world']['locations'][0]['extra']=1
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_duplicate_world_id(self):
        d=raw(); d['world']['items'][0]['id']=d['world']['locations'][0]['id']
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_duplicate_dialogue_id(self):
        d=raw(); d['dialogue']['nodes'][1]['id']=d['dialogue']['nodes'][0]['id']
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_dangling_exit(self):
        d=raw(); d['world']['exits'][0]['to']='missing'
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_dangling_dialogue(self):
        d=raw(); d['world']['locations'][0]['dialogue']='missing'
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_dangling_item_location(self):
        d=raw(); d['world']['items'][0]['location']='missing'
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_oversize(self):
        with self.assertRaises(StoryworldImportError): load_storyworld(b' ' * (8*1024*1024+1))
    def test_bad_utf8(self):
        with self.assertRaises(StoryworldImportError): load_storyworld(b'\xff')
    def test_duplicate_key(self):
        with self.assertRaises(StoryworldImportError): load_storyworld(b'{"version":1,"version":1,"world":{},"dialogue":{}}')
    def test_missing_field(self):
        d=raw(); del d['world']['locations'][0]['id']
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_bad_id(self):
        d=raw(); d['world']['locations'][0]['id']=' x'
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_wrong_type(self):
        d=raw(); d['world']['locations'][0]['name']=7
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_declarative_bad_variable(self):
        d=raw(); d['world']['exits'][0]['condition']['variable']['name']='missing'
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_declarative_bad_literal(self):
        d=raw(); d['world']['exits'][0]['condition']['value']=7
        with self.assertRaises(StoryworldImportError): load_storyworld(json.dumps(d).encode())
    def test_warning_opaque_script(self):
        d=load_storyworld(ROOT / 'samples' / 'pg03-compatible.storyworld')
        self.assertTrue(any('opaque_script' in w for w in d.warnings))


class PlanTests(unittest.TestCase):
    def setUp(self): self.document=load_storyworld(SAMPLE)
    def test_create(self): self.assertEqual(len(compute_plan(self.document).create), 4)
    def test_unchanged(self):
        p=compute_plan(self.document); current={s.id:{'kind':s.kind,'data':[s.fingerprint],'label':s.label,'transform':s.transform} for s in p.desired}
        self.assertEqual(len(compute_plan(self.document,current).unchanged),4)
    def test_update(self):
        p=compute_plan(self.document); current={s.id:{'kind':s.kind,'data':['old'],'label':s.label,'transform':s.transform} for s in p.desired}
        self.assertEqual(len(compute_plan(self.document,current).update),4)
    def test_orphan(self):
        p=compute_plan(self.document, {'gone':{'kind':'item','data':[],'label':'','transform':(0,0,0)}}); self.assertEqual(p.orphans,(('item','gone'),))
    def test_grid(self):
        specs=compute_plan(self.document).desired; self.assertEqual(specs[1].transform[0],1000.0)
    def test_exit_midpoint(self):
        exit_spec=next(s for s in compute_plan(self.document).desired if s.kind=='exit'); self.assertEqual(exit_spec.transform[0],500.0)


class ApplyTests(unittest.TestCase):
    def setUp(self):
        import unreal
        unreal.get_editor_subsystem(unreal.EditorActorSubsystem).actors.clear()
        Path(unreal.Paths.project_saved_dir()).mkdir(parents=True, exist_ok=True)
        self.doc=load_storyworld(SAMPLE)
    def test_first_import(self): self.assertEqual(len(apply_document(self.doc).created),4)
    def test_second_import_unchanged(self): apply_document(self.doc); self.assertEqual(len(apply_document(self.doc).unchanged),4)
    def test_update_no_duplicate(self):
        apply_document(self.doc); d=copy.deepcopy(self.doc.data); d['world']['locations'][0]['name']='Changed'; changed=load_storyworld(json.dumps(d).encode()); r=apply_document(changed); self.assertEqual(r.updated,['room']); self.assertEqual(len(__import__('unreal').get_editor_subsystem(__import__('unreal').EditorActorSubsystem).actors),4)
    def test_orphan_report(self):
        apply_document(self.doc); d=copy.deepcopy(self.doc.data); d['world']['items']=[]; r=apply_document(load_storyworld(json.dumps(d).encode())); self.assertEqual(r.orphans,['token'])
    def test_delete_orphan(self):
        apply_document(self.doc); d=copy.deepcopy(self.doc.data); d['world']['items']=[]; r=apply_document(load_storyworld(json.dumps(d).encode()), delete_orphans=True); self.assertEqual(r.orphans,['token']); self.assertEqual(len(__import__('unreal').get_editor_subsystem(__import__('unreal').EditorActorSubsystem).actors),3)
    def test_dry_run(self):
        r=apply_document(self.doc, dry_run=True); self.assertEqual(len(r.created),4); self.assertEqual(len(__import__('unreal').get_editor_subsystem(__import__('unreal').EditorActorSubsystem).actors),0)
    def test_api_dry_run(self): self.assertEqual(len(import_storyworld(SAMPLE,dry_run=True).created),4)
    def test_tags(self):
        apply_document(self.doc); actor=__import__('unreal').get_editor_subsystem(__import__('unreal').EditorActorSubsystem).actors[0]; self.assertTrue(any(str(t).startswith('storyworld:id=') for t in actor.tags))


class ReportTests(unittest.TestCase):
    def test_json_deterministic(self):
        a=ImportReport(created=['z','a'],warnings=['w']); b=ImportReport(created=['a','z'],warnings=['w']); self.assertEqual(a.to_json(),b.to_json()); self.assertEqual(a.to_json(), '{"created":["a","z"],"errors":[],"orphans":[],"unchanged":[],"updated":[],"warnings":["w"]}')
    def test_errors_sorted(self): self.assertEqual(ImportReport(errors=['a','b']).to_json(), ImportReport(errors=['b','a']).to_json())


if __name__ == '__main__': unittest.main()
