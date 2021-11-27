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

ttest_results = []
for exp in csv.experiment.unique():
    experiment = csv[csv['experiment'] == exp]

    baseline = experiment[experiment['variant'] == 'baseline']
    comparison = experiment[experiment['variant'] == 'comparison']

    res = scipy.stats.ttest_ind(baseline['value'], comparison['value'])
    ttest_results.append({'experiment': exp, 't-statistic': res.statistic, 'p-value': res.pvalue })

ttest_results = pd.DataFrame.from_records(ttest_results)
print(ttest_results.to_csv(index=False))
