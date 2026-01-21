Increased the number of buckets in internal histograms to reduce the smallest
bucket down to approximately 0.000244 (2.0^-12). Since this shifts all the
bucket values out, it may break VRL scripts that rely on the previous values.

authors: bruceg
