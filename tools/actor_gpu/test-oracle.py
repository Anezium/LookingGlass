import copy,unittest
import host_oracle as h
class Oracle(unittest.TestCase):
    def test_anchor_row_without_render_metrics(self):
        self.assertEqual(h.normalize(dict(frame=0,render=None)),dict(frame=0,render=None,counters={}))
    def test_only_observers_removed(self):
        a=dict(frame=1,state_sha256='abc',actor_skin=[1],submitted_packets=[dict(gpu_slot=2,vertices='a')],counters=dict(work=2,dynamic_colliders_tested=3,actor_mem_bytes=42),render={'actors':dict(cpu_ns=3,draws=4)})
        b=copy.deepcopy(a);b['submitted_packets'][0]['gpu_slot']=7;b['counters']['dynamic_colliders_tested']=5;b['render']['actors']['cpu_ns']=9
        self.assertEqual(h.normalize(a),h.normalize(b))
        for field,value in [('state_sha256','def'),('actor_skin',[2]),('future_payload','must compare')]:
            mutant=copy.deepcopy(b);mutant[field]=value
            self.assertNotEqual(h.normalize(a),h.normalize(mutant))
        for mutant in [copy.deepcopy(b) for _ in range(3)]:
            mutant['counters']['work']=3
            self.assertNotEqual(h.normalize(a),h.normalize(mutant))
        b['submitted_packets'][0]['vertices']='changed';self.assertNotEqual(h.normalize(a),h.normalize(b))
    def test_pixel_and_event_mutants_rejected(self):
        a=dict(label='base',tape='t',frames=1,signatures=[[1,'a']],images=['pixel'],presentation=['present'],events=['ordered'],replay={'events':0},done='done',binary_sha256='a')
        for field in ('frames','signatures','images','presentation','events','replay'):
            b=copy.deepcopy(a);b['label']='mutant';b[field]='changed'
            with self.assertRaises(SystemExit):h.compare(a,b)
if __name__=='__main__':unittest.main()
