"""Summarize any number of CSVs, retaining unavailable counts/residuals."""
import argparse,json
from light_csv import read,summarize
p=argparse.ArgumentParser();p.add_argument('csv',nargs='+');p.add_argument('--legacy-observers',choices=['direct','observed']);a=p.parse_args()
print(json.dumps({f:summarize(read(f,a.legacy_observers)) for f in a.csv},indent=2))
