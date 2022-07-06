import gc
import glob
import math
import numpy as np
import os
import pandas as pd
import sys

def file_is_empty(path):
    return os.path.exists(path) and os.stat(path).st_size == 0

# Opens a single capture file, filtering as needed
def open_capture(capture_path, metric_name, unwanted_labels):
    print("[open_capture] reading: {}".format(capture_path), file=sys.stderr)
    with pd.read_json(capture_path, lines=True, chunksize=16384) as reader:
        for chunk in reader:
            # Drop unwanted labels from the capture file. The more data we
            # can shed here the less we have to hold in memory overall.
            chunk = chunk[chunk.metric_name == metric_name]
            chunk = chunk.drop(labels=unwanted_labels, axis=1)
            if chunk.empty:
                continue
            yield chunk
            gc.collect()

# Opens our capture files, filtering as needed
#
# The capture files generated in our experiments can be quite large, relative to
# the CI machine memory we have available, and we need to do a fair bit here to
# ensure everything will fit into memory. We primarily achieve this by garbage
# collecting after each read, filtering out columns this program does not need
# and reading small chunks of each capture file at a time.
def open_captures(capture_dir, metric_name, unwanted_labels):
    capture_paths = glob.glob(os.path.join(capture_dir, "**/*.captures"), recursive=True)
    for f in capture_paths:
        if file_is_empty(f):
            print("[open_captures] encountered empty capture file, skipping: {}".format(f), file=sys.stderr)
            continue
        yield pd.concat(open_capture(f, metric_name, unwanted_labels))

def compute_throughput(captures, **kwargs):
    cpus = kwargs.get('cpus', 1)
    for capture in captures:
        # Scale bytes_written down to our per-CPU unit, then compute and
        # introduce throughput into the table. We compute throughput by central
        # finite difference, using the time value recorded to understand step
        # size between samples.
        capture.value = capture.value.div(cpus)
        # The fetches performed by lading and are 1000 milliseconds apart with
        # `fetch_index` marking each poll.
        capture['throughput'] = np.gradient(capture.value, capture.fetch_index) # bytes/second/cpu
        yield capture

def human_bytes(b):
    is_negative = False
    if b < 0:
        is_negative = True
        b = -b
    if b < 1 and b >= 0:
        return "0B"
    names = ("B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB")
    i = int(math.floor(math.log(b, 1024)))
    p = math.pow(1024, i)
    s = round(b / p, 2)
    if is_negative:
        s = -s
    return "%s%s" % (s, names[i])

# Use Tukey's method to detect values that sit 1.5 times outside the IQR.
def total_outliers(df):
    q1 = df['value'].quantile(0.25)
    q3 = df['value'].quantile(0.75)
    iqr = q3 - q1
    scaled_iqr = 1.5 * iqr

    outside_range = lambda b: (b < (q1 - scaled_iqr)) or (b > (q3 + scaled_iqr))
    return df['value'].apply(outside_range).sum()

def confidence(p):
    c = (1.0 - p) * 100
    return "{confidence:.{digits}f}%".format(confidence=c, digits=2)
