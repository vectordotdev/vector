#!/usr/bin/env python3

import pandas as pd
import scipy.stats
import argparse

parser = argparse.ArgumentParser(description='t-test experiments with Welch method')
parser.add_argument('captures', type=str, help='the captures csv to analyze')
parser.add_argument('warmup_seconds', type=int, help='the number of seconds to treat as warmup')
args = parser.parse_args()

csv = pd.read_csv(args.captures)
fetch_index_past_warmup = csv['fetch_index'] > args.warmup_seconds
csv = csv[fetch_index_past_warmup]
res = csv.groupby(['experiment', 'variant'])['value'].describe(percentiles=[0.5, 0.75, 0.90, 0.99])
res = res.rename(columns={'50%': 'p50', '75%': 'p75', '90%': 'p90', '99%': 'p99'})
print(res.to_csv(index=False))
