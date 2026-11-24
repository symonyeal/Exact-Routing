# Metro routing benchmark

Latency tables show mean, p50, and p95 microseconds across per-query mean latencies; each query mean averages the repeated passes. Native and rounded weights are separate. Rankings use p50 only.
Native weights are travel seconds. Rounded weights are max(1, round(1000 × seconds)).
All adapters return distances. CH internally reconstructs a path through its public API; that extra work is included. These are implementation timings, not isolated search kernels or equivalent path-output timings.

## Native weights

| City | Nodes | Method | Mean µs | p50 µs | p95 µs |
|---|---:|---|---:|---:|---:|
| busan | 157,831 | Graph-1024 | 253.77 | 43.59 | 820.75 |
| busan | 157,831 | Graph-256 | 506.85 | 38.03 | 1812.94 |
| busan | 157,831 | Graph-4096 | 285.42 | 58.50 | 766.71 |
| busan | 157,831 | Hypergraph-1024 | 161.75 | 39.13 | 495.59 |
| busan | 157,831 | Hypergraph-256 | 284.72 | 31.05 | 990.09 |
| busan | 157,831 | Hypergraph-4096 | 240.74 | 60.63 | 657.42 |
| busan | 157,831 | bidirectional | 2553.87 | 176.31 | 11668.80 |
| busan | 157,831 | dijkstra | 3406.96 | 366.04 | 12576.31 |
| daegu | 153,887 | Graph-1024 | 295.70 | 24.79 | 981.52 |
| daegu | 153,887 | Graph-256 | 579.73 | 34.69 | 2079.35 |
| daegu | 153,887 | Graph-4096 | 301.85 | 29.20 | 910.10 |
| daegu | 153,887 | Hypergraph-1024 | 195.06 | 22.04 | 617.87 |
| daegu | 153,887 | Hypergraph-256 | 338.93 | 27.76 | 1209.84 |
| daegu | 153,887 | Hypergraph-4096 | 266.73 | 31.13 | 841.67 |
| daegu | 153,887 | bidirectional | 4522.06 | 323.30 | 19651.31 |
| daegu | 153,887 | dijkstra | 3681.66 | 492.73 | 13858.54 |
| daejeon | 95,765 | Graph-1024 | 219.74 | 50.35 | 630.45 |
| daejeon | 95,765 | Graph-256 | 364.87 | 60.73 | 1163.84 |
| daejeon | 95,765 | Graph-4096 | 306.27 | 60.05 | 847.29 |
| daejeon | 95,765 | Hypergraph-1024 | 146.82 | 43.42 | 407.14 |
| daejeon | 95,765 | Hypergraph-256 | 205.65 | 38.18 | 637.02 |
| daejeon | 95,765 | Hypergraph-4096 | 265.53 | 59.14 | 738.18 |
| daejeon | 95,765 | bidirectional | 1394.21 | 144.69 | 5435.54 |
| daejeon | 95,765 | dijkstra | 2253.28 | 329.10 | 7847.05 |
| gwangju | 132,467 | Graph-1024 | 263.13 | 32.67 | 834.09 |
| gwangju | 132,467 | Graph-256 | 465.71 | 30.31 | 1629.74 |
| gwangju | 132,467 | Graph-4096 | 366.17 | 38.50 | 1035.08 |
| gwangju | 132,467 | Hypergraph-1024 | 169.84 | 33.42 | 501.76 |
| gwangju | 132,467 | Hypergraph-256 | 258.14 | 27.83 | 885.87 |
| gwangju | 132,467 | Hypergraph-4096 | 318.65 | 39.61 | 883.79 |
| gwangju | 132,467 | bidirectional | 2282.66 | 130.29 | 9190.21 |
| gwangju | 132,467 | dijkstra | 3729.36 | 328.41 | 13382.23 |
| incheon | 247,862 | Graph-1024 | 129.93 | 15.30 | 529.59 |
| incheon | 247,862 | Graph-256 | 241.51 | 11.89 | 1055.28 |
| incheon | 247,862 | Graph-4096 | 152.20 | 17.17 | 490.60 |
| incheon | 247,862 | Hypergraph-1024 | 84.36 | 15.15 | 320.40 |
| incheon | 247,862 | Hypergraph-256 | 138.49 | 10.02 | 606.19 |
| incheon | 247,862 | Hypergraph-4096 | 136.19 | 18.17 | 447.47 |
| incheon | 247,862 | bidirectional | 3291.60 | 120.91 | 14955.84 |
| incheon | 247,862 | dijkstra | 2047.16 | 113.62 | 7793.15 |
| london-large | 50,301 | Graph-1024 | 131.76 | 24.08 | 372.00 |
| london-large | 50,301 | Graph-256 | 176.45 | 26.60 | 578.40 |
| london-large | 50,301 | Graph-4096 | 223.28 | 29.29 | 611.53 |
| london-large | 50,301 | Hypergraph-1024 | 92.15 | 25.48 | 246.38 |
| london-large | 50,301 | Hypergraph-256 | 102.03 | 20.81 | 319.22 |
| london-large | 50,301 | Hypergraph-4096 | 217.62 | 30.02 | 602.91 |
| london-large | 50,301 | bidirectional | 699.34 | 56.71 | 2738.72 |
| london-large | 50,301 | dijkstra | 962.07 | 109.44 | 3380.33 |
| new_york-large | 50,306 | Graph-1024 | 249.12 | 63.78 | 788.26 |
| new_york-large | 50,306 | Graph-256 | 451.40 | 52.78 | 1580.37 |
| new_york-large | 50,306 | Graph-4096 | 269.34 | 72.18 | 794.28 |
| new_york-large | 50,306 | Hypergraph-1024 | 157.75 | 48.82 | 469.24 |
| new_york-large | 50,306 | Hypergraph-256 | 245.88 | 43.26 | 827.48 |
| new_york-large | 50,306 | Hypergraph-4096 | 222.75 | 67.53 | 627.14 |
| new_york-large | 50,306 | bidirectional | 722.58 | 43.00 | 2690.11 |
| new_york-large | 50,306 | dijkstra | 915.58 | 77.87 | 3291.29 |
| sejong | 85,647 | Graph-1024 | 80.80 | 9.55 | 247.76 |
| sejong | 85,647 | Graph-256 | 117.81 | 9.32 | 427.71 |
| sejong | 85,647 | Graph-4096 | 158.15 | 15.14 | 457.01 |
| sejong | 85,647 | Hypergraph-1024 | 61.85 | 8.73 | 182.81 |
| sejong | 85,647 | Hypergraph-256 | 70.15 | 7.85 | 251.56 |
| sejong | 85,647 | Hypergraph-4096 | 153.63 | 17.58 | 461.43 |
| sejong | 85,647 | bidirectional | 1038.67 | 58.77 | 5192.67 |
| sejong | 85,647 | dijkstra | 1123.52 | 93.53 | 4180.50 |
| seoul | 328,610 | Graph-1024 | 686.24 | 43.65 | 2375.83 |
| seoul | 328,610 | Graph-256 | 1427.99 | 71.79 | 5239.14 |
| seoul | 328,610 | Graph-4096 | 501.49 | 37.14 | 1491.59 |
| seoul | 328,610 | Hypergraph-1024 | 412.52 | 35.37 | 1377.91 |
| seoul | 328,610 | Hypergraph-256 | 809.52 | 43.43 | 2867.40 |
| seoul | 328,610 | Hypergraph-4096 | 382.16 | 38.43 | 1083.38 |
| seoul | 328,610 | bidirectional | 4701.69 | 256.75 | 19313.35 |
| seoul | 328,610 | dijkstra | 7659.20 | 750.28 | 28109.58 |
| tokyo-large | 50,231 | Graph-1024 | 211.35 | 43.98 | 672.67 |
| tokyo-large | 50,231 | Graph-256 | 315.93 | 36.35 | 1123.77 |
| tokyo-large | 50,231 | Graph-4096 | 275.49 | 45.88 | 792.89 |
| tokyo-large | 50,231 | Hypergraph-1024 | 144.14 | 45.48 | 429.59 |
| tokyo-large | 50,231 | Hypergraph-256 | 184.18 | 32.63 | 625.37 |
| tokyo-large | 50,231 | Hypergraph-4096 | 233.15 | 41.31 | 663.84 |
| tokyo-large | 50,231 | bidirectional | 648.78 | 26.37 | 2485.56 |
| tokyo-large | 50,231 | dijkstra | 903.80 | 49.21 | 3295.25 |
| ulsan | 146,901 | Graph-1024 | 157.15 | 23.94 | 489.77 |
| ulsan | 146,901 | Graph-256 | 283.64 | 19.45 | 1020.84 |
| ulsan | 146,901 | Graph-4096 | 229.51 | 21.43 | 659.44 |
| ulsan | 146,901 | Hypergraph-1024 | 103.51 | 22.65 | 299.79 |
| ulsan | 146,901 | Hypergraph-256 | 158.22 | 14.92 | 538.63 |
| ulsan | 146,901 | Hypergraph-4096 | 205.40 | 21.05 | 608.70 |
| ulsan | 146,901 | bidirectional | 1965.74 | 100.97 | 8106.20 |
| ulsan | 146,901 | dijkstra | 2942.51 | 275.28 | 11078.27 |

## Rounded weights

| City | Nodes | Method | Mean µs | p50 µs | p95 µs |
|---|---:|---|---:|---:|---:|
| busan | 157,831 | Graph-1024 | 253.08 | 44.65 | 819.78 |
| busan | 157,831 | Graph-256 | 503.70 | 42.07 | 1796.22 |
| busan | 157,831 | Graph-4096 | 286.18 | 58.58 | 769.44 |
| busan | 157,831 | Hypergraph-1024 | 161.33 | 38.53 | 492.28 |
| busan | 157,831 | Hypergraph-256 | 285.08 | 36.00 | 990.15 |
| busan | 157,831 | Hypergraph-4096 | 241.05 | 56.16 | 655.25 |
| busan | 157,831 | bidirectional | 2425.91 | 174.91 | 11158.80 |
| busan | 157,831 | dijkstra | 3436.81 | 374.63 | 12734.47 |
| busan | 157,831 | fast_paths-CH | 36.58 | 19.46 | 92.99 |
| daegu | 153,887 | Graph-1024 | 282.40 | 25.21 | 943.48 |
| daegu | 153,887 | Graph-256 | 543.02 | 33.42 | 1974.52 |
| daegu | 153,887 | Graph-4096 | 291.39 | 30.33 | 899.89 |
| daegu | 153,887 | Hypergraph-1024 | 187.67 | 21.54 | 603.42 |
| daegu | 153,887 | Hypergraph-256 | 320.05 | 25.35 | 1155.25 |
| daegu | 153,887 | Hypergraph-4096 | 257.80 | 30.93 | 797.35 |
| daegu | 153,887 | bidirectional | 4300.19 | 297.07 | 18911.63 |
| daegu | 153,887 | dijkstra | 3535.97 | 457.30 | 13293.48 |
| daegu | 153,887 | fast_paths-CH | 39.58 | 17.22 | 115.29 |
| daejeon | 95,765 | Graph-1024 | 216.59 | 47.13 | 620.26 |
| daejeon | 95,765 | Graph-256 | 353.53 | 61.86 | 1124.79 |
| daejeon | 95,765 | Graph-4096 | 302.59 | 61.67 | 844.21 |
| daejeon | 95,765 | Hypergraph-1024 | 146.91 | 44.27 | 400.48 |
| daejeon | 95,765 | Hypergraph-256 | 205.14 | 41.27 | 630.74 |
| daejeon | 95,765 | Hypergraph-4096 | 264.04 | 58.77 | 716.26 |
| daejeon | 95,765 | bidirectional | 1404.39 | 143.20 | 5478.50 |
| daejeon | 95,765 | dijkstra | 2233.00 | 334.92 | 7791.58 |
| daejeon | 95,765 | fast_paths-CH | 37.03 | 18.51 | 92.44 |
| gwangju | 132,467 | Graph-1024 | 278.17 | 39.35 | 881.57 |
| gwangju | 132,467 | Graph-256 | 486.45 | 30.78 | 1696.47 |
| gwangju | 132,467 | Graph-4096 | 370.54 | 43.49 | 1042.40 |
| gwangju | 132,467 | Hypergraph-1024 | 175.92 | 32.93 | 514.29 |
| gwangju | 132,467 | Hypergraph-256 | 270.81 | 27.10 | 919.44 |
| gwangju | 132,467 | Hypergraph-4096 | 323.47 | 42.35 | 890.95 |
| gwangju | 132,467 | bidirectional | 2333.53 | 135.76 | 9612.08 |
| gwangju | 132,467 | dijkstra | 3794.39 | 356.69 | 13675.29 |
| gwangju | 132,467 | fast_paths-CH | 42.30 | 22.19 | 107.14 |
| incheon | 247,862 | Graph-1024 | 161.44 | 16.60 | 663.75 |
| incheon | 247,862 | Graph-256 | 312.87 | 16.94 | 1374.08 |
| incheon | 247,862 | Graph-4096 | 172.72 | 18.11 | 570.96 |
| incheon | 247,862 | Hypergraph-1024 | 106.04 | 17.04 | 409.95 |
| incheon | 247,862 | Hypergraph-256 | 184.21 | 11.80 | 787.77 |
| incheon | 247,862 | Hypergraph-4096 | 151.91 | 22.08 | 492.84 |
| incheon | 247,862 | bidirectional | 3808.71 | 214.99 | 17519.89 |
| incheon | 247,862 | dijkstra | 2367.19 | 188.38 | 9133.40 |
| incheon | 247,862 | fast_paths-CH | 21.30 | 11.09 | 74.52 |
| london-large | 50,301 | Graph-1024 | 129.40 | 25.37 | 363.41 |
| london-large | 50,301 | Graph-256 | 177.74 | 29.33 | 578.27 |
| london-large | 50,301 | Graph-4096 | 224.11 | 32.47 | 618.82 |
| london-large | 50,301 | Hypergraph-1024 | 92.49 | 21.83 | 252.75 |
| london-large | 50,301 | Hypergraph-256 | 102.25 | 20.54 | 320.80 |
| london-large | 50,301 | Hypergraph-4096 | 218.84 | 31.66 | 605.35 |
| london-large | 50,301 | bidirectional | 710.34 | 59.49 | 2780.46 |
| london-large | 50,301 | dijkstra | 973.63 | 114.69 | 3406.35 |
| london-large | 50,301 | fast_paths-CH | 28.95 | 16.71 | 68.20 |
| new_york-large | 50,306 | Graph-1024 | 257.93 | 68.36 | 827.98 |
| new_york-large | 50,306 | Graph-256 | 451.38 | 55.14 | 1582.05 |
| new_york-large | 50,306 | Graph-4096 | 276.14 | 73.50 | 815.70 |
| new_york-large | 50,306 | Hypergraph-1024 | 167.80 | 49.53 | 514.47 |
| new_york-large | 50,306 | Hypergraph-256 | 259.06 | 45.44 | 884.27 |
| new_york-large | 50,306 | Hypergraph-4096 | 231.38 | 67.36 | 661.67 |
| new_york-large | 50,306 | bidirectional | 723.58 | 42.00 | 2656.58 |
| new_york-large | 50,306 | dijkstra | 924.12 | 77.76 | 3328.57 |
| new_york-large | 50,306 | fast_paths-CH | 49.00 | 35.31 | 116.56 |
| sejong | 85,647 | Graph-1024 | 63.21 | 6.66 | 187.42 |
| sejong | 85,647 | Graph-256 | 84.29 | 6.80 | 280.53 |
| sejong | 85,647 | Graph-4096 | 128.27 | 8.27 | 378.40 |
| sejong | 85,647 | Hypergraph-1024 | 48.22 | 7.28 | 137.88 |
| sejong | 85,647 | Hypergraph-256 | 47.66 | 6.62 | 154.76 |
| sejong | 85,647 | Hypergraph-4096 | 124.06 | 8.93 | 369.17 |
| sejong | 85,647 | bidirectional | 842.00 | 28.02 | 4160.46 |
| sejong | 85,647 | dijkstra | 1030.60 | 49.35 | 3824.04 |
| sejong | 85,647 | fast_paths-CH | 17.19 | 6.37 | 51.58 |
| seoul | 328,610 | Graph-1024 | 673.12 | 40.19 | 2319.27 |
| seoul | 328,610 | Graph-256 | 1402.06 | 75.73 | 5013.73 |
| seoul | 328,610 | Graph-4096 | 494.83 | 42.98 | 1477.16 |
| seoul | 328,610 | Hypergraph-1024 | 407.57 | 38.18 | 1352.19 |
| seoul | 328,610 | Hypergraph-256 | 793.88 | 42.81 | 2811.37 |
| seoul | 328,610 | Hypergraph-4096 | 375.15 | 36.84 | 1070.72 |
| seoul | 328,610 | bidirectional | 4611.00 | 258.78 | 18667.19 |
| seoul | 328,610 | dijkstra | 7747.43 | 804.15 | 28379.73 |
| seoul | 328,610 | fast_paths-CH | 58.82 | 26.42 | 149.16 |
| tokyo-large | 50,231 | Graph-1024 | 173.72 | 35.17 | 555.89 |
| tokyo-large | 50,231 | Graph-256 | 260.74 | 30.10 | 925.25 |
| tokyo-large | 50,231 | Graph-4096 | 234.45 | 38.42 | 682.69 |
| tokyo-large | 50,231 | Hypergraph-1024 | 118.67 | 35.40 | 350.75 |
| tokyo-large | 50,231 | Hypergraph-256 | 149.99 | 26.72 | 512.48 |
| tokyo-large | 50,231 | Hypergraph-4096 | 197.52 | 35.06 | 564.33 |
| tokyo-large | 50,231 | bidirectional | 548.58 | 18.08 | 2116.24 |
| tokyo-large | 50,231 | dijkstra | 830.73 | 39.07 | 3016.45 |
| tokyo-large | 50,231 | fast_paths-CH | 20.05 | 14.52 | 45.58 |
| ulsan | 146,901 | Graph-1024 | 158.06 | 20.82 | 496.09 |
| ulsan | 146,901 | Graph-256 | 280.58 | 17.99 | 1005.90 |
| ulsan | 146,901 | Graph-4096 | 229.27 | 23.59 | 661.34 |
| ulsan | 146,901 | Hypergraph-1024 | 105.00 | 21.01 | 303.17 |
| ulsan | 146,901 | Hypergraph-256 | 160.56 | 14.22 | 560.70 |
| ulsan | 146,901 | Hypergraph-4096 | 204.89 | 20.66 | 601.83 |
| ulsan | 146,901 | bidirectional | 1988.79 | 104.20 | 8228.95 |
| ulsan | 146,901 | dijkstra | 2944.00 | 279.00 | 11164.71 |
| ulsan | 146,901 | fast_paths-CH | 32.97 | 11.84 | 92.36 |

## Best-method comparison

Rankings use p50. Dijkstra ratios divide Dijkstra p50 by the fastest method p50 at the same scale. H/CH uses the selected best-p50 hypergraph method's paired median of per-query mean-latency ratios.
| City | Native fastest | Native Dijkstra / best | Rounded fastest | Rounded Dijkstra / best | Rounded paired H / CH |
|---|---|---:|---|---:|---:|
| busan | Hypergraph-256 | 11.79x | fast_paths-CH | 19.25x | 3.54x |
| daegu | Hypergraph-1024 | 22.36x | fast_paths-CH | 26.56x | 2.38x |
| daejeon | Hypergraph-256 | 8.62x | fast_paths-CH | 18.09x | 4.03x |
| gwangju | Hypergraph-256 | 11.80x | fast_paths-CH | 16.08x | 3.09x |
| incheon | Hypergraph-256 | 11.34x | fast_paths-CH | 16.99x | 2.11x |
| london-large | Hypergraph-256 | 5.26x | fast_paths-CH | 6.86x | 2.34x |
| new_york-large | bidirectional | 1.81x | fast_paths-CH | 2.20x | 2.45x |
| sejong | Hypergraph-256 | 11.91x | fast_paths-CH | 7.75x | 1.61x |
| seoul | Hypergraph-1024 | 21.21x | fast_paths-CH | 30.44x | 4.97x |
| tokyo-large | bidirectional | 1.87x | fast_paths-CH | 2.69x | 2.42x |
| ulsan | Hypergraph-256 | 18.44x | fast_paths-CH | 23.57x | 2.13x |

## Throughput scaling

Queries per second; each throughput sample includes worker launch and join time but excludes worker-state allocation. Ratios are descriptive; losses remain visible.
| City | Metric | Method | 1 worker QPS | Requested workers QPS | Parallel / serial |
|---|---|---|---:|---:|---:|
| busan | native | Graph-1024 | 5537.7 | 10906.9 | 1.97× |
| busan | native | Graph-256 | 2838.6 | 5451.2 | 1.92× |
| busan | native | Graph-4096 | 4356.2 | 8910.1 | 2.05× |
| busan | native | Hypergraph-1024 | 8832.1 | 17922.2 | 2.03× |
| busan | native | Hypergraph-256 | 5469.6 | 10542.8 | 1.93× |
| busan | native | Hypergraph-4096 | 5201.4 | 10674.6 | 2.05× |
| busan | native | bidirectional | 461.1 | 892.5 | 1.94× |
| busan | native | dijkstra | 317.2 | 616.3 | 1.94× |
| busan | rounded | Graph-1024 | 5628.7 | 11086.6 | 1.97× |
| busan | rounded | Graph-256 | 2918.6 | 5568.0 | 1.91× |
| busan | rounded | Graph-4096 | 4380.1 | 8928.7 | 2.04× |
| busan | rounded | Hypergraph-1024 | 9204.1 | 18172.3 | 1.97× |
| busan | rounded | Hypergraph-256 | 5570.4 | 10713.6 | 1.92× |
| busan | rounded | Hypergraph-4096 | 5245.3 | 10807.4 | 2.06× |
| busan | rounded | bidirectional | 458.7 | 821.5 | 1.79× |
| busan | rounded | dijkstra | 316.3 | 615.0 | 1.94× |
| busan | rounded | fast_paths-CH | 74329.8 | 170412.6 | 2.29× |
| daegu | native | Graph-1024 | 5348.0 | 10627.1 | 1.99× |
| daegu | native | Graph-256 | 3031.2 | 6051.7 | 2.00× |
| daegu | native | Graph-4096 | 4005.0 | 8470.1 | 2.11× |
| daegu | native | Hypergraph-1024 | 7908.9 | 15913.4 | 2.01× |
| daegu | native | Hypergraph-256 | 5618.0 | 11201.7 | 1.99× |
| daegu | native | Hypergraph-4096 | 4591.5 | 9882.3 | 2.15× |
| daegu | native | bidirectional | 223.8 | 424.7 | 1.90× |
| daegu | native | dijkstra | 269.5 | 614.7 | 2.28× |
| daegu | rounded | Graph-1024 | 5469.8 | 11089.3 | 2.03× |
| daegu | rounded | Graph-256 | 3108.2 | 6251.2 | 2.01× |
| daegu | rounded | Graph-4096 | 4555.3 | 8859.3 | 1.94× |
| daegu | rounded | Hypergraph-1024 | 7860.5 | 16228.9 | 2.06× |
| daegu | rounded | Hypergraph-256 | 5711.6 | 11495.4 | 2.01× |
| daegu | rounded | Hypergraph-4096 | 4368.3 | 9012.9 | 2.06× |
| daegu | rounded | bidirectional | 232.9 | 475.4 | 2.04× |
| daegu | rounded | dijkstra | 297.7 | 674.3 | 2.26× |
| daegu | rounded | fast_paths-CH | 71780.7 | 136862.9 | 1.91× |
| daejeon | native | Graph-1024 | 6705.4 | 13264.2 | 1.98× |
| daejeon | native | Graph-256 | 4314.8 | 8187.3 | 1.90× |
| daejeon | native | Graph-4096 | 4105.6 | 8100.1 | 1.97× |
| daejeon | native | Hypergraph-1024 | 10006.3 | 19692.4 | 1.97× |
| daejeon | native | Hypergraph-256 | 8305.6 | 15839.7 | 1.91× |
| daejeon | native | Hypergraph-4096 | 4567.7 | 9124.7 | 2.00× |
| daejeon | native | bidirectional | 828.4 | 1601.3 | 1.93× |
| daejeon | native | dijkstra | 485.3 | 957.8 | 1.97× |
| daejeon | rounded | Graph-1024 | 6814.1 | 13168.8 | 1.93× |
| daejeon | rounded | Graph-256 | 4380.7 | 8389.1 | 1.91× |
| daejeon | rounded | Graph-4096 | 4094.7 | 8186.9 | 2.00× |
| daejeon | rounded | Hypergraph-1024 | 10044.3 | 19780.4 | 1.97× |
| daejeon | rounded | Hypergraph-256 | 8349.2 | 16327.8 | 1.96× |
| daejeon | rounded | Hypergraph-4096 | 4622.6 | 9176.6 | 1.99× |
| daejeon | rounded | bidirectional | 825.9 | 1138.2 | 1.38× |
| daejeon | rounded | dijkstra | 483.9 | 952.7 | 1.97× |
| daejeon | rounded | fast_paths-CH | 74624.0 | 147866.3 | 1.98× |
| gwangju | native | Graph-1024 | 5170.3 | 10289.7 | 1.99× |
| gwangju | native | Graph-256 | 3082.9 | 6111.3 | 1.98× |
| gwangju | native | Graph-4096 | 3212.2 | 6681.5 | 2.08× |
| gwangju | native | Hypergraph-1024 | 8313.1 | 16388.5 | 1.97× |
| gwangju | native | Hypergraph-256 | 6003.0 | 11826.1 | 1.97× |
| gwangju | native | Hypergraph-4096 | 3835.8 | 7789.6 | 2.03× |
| gwangju | native | bidirectional | 468.8 | 890.7 | 1.90× |
| gwangju | native | dijkstra | 290.6 | 571.7 | 1.97× |
| gwangju | rounded | Graph-1024 | 5105.6 | 10367.4 | 2.03× |
| gwangju | rounded | Graph-256 | 3122.0 | 6142.4 | 1.97× |
| gwangju | rounded | Graph-4096 | 3292.3 | 6529.2 | 1.98× |
| gwangju | rounded | Hypergraph-1024 | 8135.4 | 16546.9 | 2.03× |
| gwangju | rounded | Hypergraph-256 | 6057.7 | 11990.2 | 1.98× |
| gwangju | rounded | Hypergraph-4096 | 3667.2 | 7552.9 | 2.06× |
| gwangju | rounded | bidirectional | 453.4 | 891.7 | 1.97× |
| gwangju | rounded | dijkstra | 279.6 | 572.4 | 2.05× |
| gwangju | rounded | fast_paths-CH | 63001.1 | 131929.5 | 2.09× |
| incheon | native | Graph-1024 | 9978.5 | 20307.1 | 2.04× |
| incheon | native | Graph-256 | 5446.8 | 10944.5 | 2.01× |
| incheon | native | Graph-4096 | 7690.6 | 15444.3 | 2.01× |
| incheon | native | Hypergraph-1024 | 17218.2 | 33577.1 | 1.95× |
| incheon | native | Hypergraph-256 | 10545.5 | 21277.4 | 2.02× |
| incheon | native | Hypergraph-4096 | 9589.5 | 18914.2 | 1.97× |
| incheon | native | bidirectional | 302.4 | 589.6 | 1.95× |
| incheon | native | dijkstra | 512.4 | 1071.2 | 2.09× |
| incheon | rounded | Graph-1024 | 10515.4 | 21846.9 | 2.08× |
| incheon | rounded | Graph-256 | 5921.3 | 11525.9 | 1.95× |
| incheon | rounded | Graph-4096 | 9040.8 | 17585.0 | 1.95× |
| incheon | rounded | Hypergraph-1024 | 16053.2 | 33793.5 | 2.11× |
| incheon | rounded | Hypergraph-256 | 11185.6 | 22089.9 | 1.97× |
| incheon | rounded | Hypergraph-4096 | 10249.1 | 20550.0 | 2.01× |
| incheon | rounded | bidirectional | 317.7 | 500.9 | 1.58× |
| incheon | rounded | dijkstra | 445.1 | 1089.2 | 2.45× |
| incheon | rounded | fast_paths-CH | 175153.9 | 319057.5 | 1.82× |
| london-large | native | Graph-1024 | 8904.3 | 17491.0 | 1.96× |
| london-large | native | Graph-256 | 6835.4 | 13448.3 | 1.97× |
| london-large | native | Graph-4096 | 4818.9 | 9479.0 | 1.97× |
| london-large | native | Hypergraph-1024 | 12286.1 | 23807.8 | 1.94× |
| london-large | native | Hypergraph-256 | 11991.9 | 23480.9 | 1.96× |
| london-large | native | Hypergraph-4096 | 4894.6 | 9623.3 | 1.97× |
| london-large | native | bidirectional | 1482.1 | 2623.6 | 1.77× |
| london-large | native | dijkstra | 1114.3 | 2197.8 | 1.97× |
| london-large | rounded | Graph-1024 | 8992.1 | 17545.5 | 1.95× |
| london-large | rounded | Graph-256 | 6957.5 | 13514.9 | 1.94× |
| london-large | rounded | Graph-4096 | 4836.3 | 9536.9 | 1.97× |
| london-large | rounded | Hypergraph-1024 | 12316.8 | 23882.8 | 1.94× |
| london-large | rounded | Hypergraph-256 | 12142.9 | 23725.8 | 1.95× |
| london-large | rounded | Hypergraph-4096 | 4911.5 | 9629.9 | 1.96× |
| london-large | rounded | bidirectional | 1485.8 | 2413.8 | 1.62× |
| london-large | rounded | dijkstra | 1119.1 | 2175.3 | 1.94× |
| london-large | rounded | fast_paths-CH | 68826.8 | 103944.1 | 1.51× |
| new_york-large | native | Graph-1024 | 4695.3 | 9315.8 | 1.98× |
| new_york-large | native | Graph-256 | 2490.2 | 4935.1 | 1.98× |
| new_york-large | native | Graph-4096 | 4177.0 | 8335.0 | 2.00× |
| new_york-large | native | Hypergraph-1024 | 7856.5 | 15461.2 | 1.97× |
| new_york-large | native | Hypergraph-256 | 4851.9 | 9566.8 | 1.97× |
| new_york-large | native | Hypergraph-4096 | 5069.7 | 10014.3 | 1.98× |
| new_york-large | native | bidirectional | 1458.9 | 2764.8 | 1.90× |
| new_york-large | native | dijkstra | 1136.5 | 2218.0 | 1.95× |
| new_york-large | rounded | Graph-1024 | 4861.5 | 9562.8 | 1.97× |
| new_york-large | rounded | Graph-256 | 2593.9 | 5106.3 | 1.97× |
| new_york-large | rounded | Graph-4096 | 4235.5 | 8396.9 | 1.98× |
| new_york-large | rounded | Hypergraph-1024 | 7889.3 | 15313.3 | 1.94× |
| new_york-large | rounded | Hypergraph-256 | 4905.6 | 9609.2 | 1.96× |
| new_york-large | rounded | Hypergraph-4096 | 5065.5 | 9984.0 | 1.97× |
| new_york-large | rounded | bidirectional | 1521.3 | 3003.4 | 1.97× |
| new_york-large | rounded | dijkstra | 1130.7 | 2212.7 | 1.96× |
| new_york-large | rounded | fast_paths-CH | 57057.1 | 113756.4 | 1.99× |
| sejong | native | Graph-1024 | 19807.9 | 35312.5 | 1.78× |
| sejong | native | Graph-256 | 11452.6 | 26884.7 | 2.35× |
| sejong | native | Graph-4096 | 9029.0 | 17237.6 | 1.91× |
| sejong | native | Hypergraph-1024 | 25924.6 | 50230.7 | 1.94× |
| sejong | native | Hypergraph-256 | 25389.9 | 50071.3 | 1.97× |
| sejong | native | Hypergraph-4096 | 8858.6 | 18257.0 | 2.06× |
| sejong | native | bidirectional | 1207.6 | 1884.2 | 1.56× |
| sejong | native | dijkstra | 1023.1 | 1920.4 | 1.88× |
| sejong | rounded | Graph-1024 | 19520.5 | 39364.4 | 2.02× |
| sejong | rounded | Graph-256 | 13838.9 | 26605.0 | 1.92× |
| sejong | rounded | Graph-4096 | 9068.4 | 17198.1 | 1.90× |
| sejong | rounded | Hypergraph-1024 | 26870.7 | 52059.6 | 1.94× |
| sejong | rounded | Hypergraph-256 | 25879.1 | 53006.2 | 2.05× |
| sejong | rounded | Hypergraph-4096 | 9308.0 | 18662.1 | 2.00× |
| sejong | rounded | bidirectional | 1230.0 | 1771.6 | 1.44× |
| sejong | rounded | dijkstra | 1010.1 | 1970.9 | 1.95× |
| sejong | rounded | fast_paths-CH | 187421.6 | 367229.1 | 1.96× |
| seoul | native | Graph-1024 | 2675.9 | 5356.1 | 2.00× |
| seoul | native | Graph-256 | 1341.8 | 2697.5 | 2.01× |
| seoul | native | Graph-4096 | 3000.5 | 5783.8 | 1.93× |
| seoul | native | Hypergraph-1024 | 4623.8 | 9551.9 | 2.07× |
| seoul | native | Hypergraph-256 | 2750.9 | 5457.0 | 1.98× |
| seoul | native | Hypergraph-4096 | 3872.6 | 8053.3 | 2.08× |
| seoul | native | bidirectional | 223.0 | 474.3 | 2.13× |
| seoul | native | dijkstra | 130.1 | 285.6 | 2.19× |
| seoul | rounded | Graph-1024 | 2800.9 | 5751.5 | 2.05× |
| seoul | rounded | Graph-256 | 1399.4 | 2674.6 | 1.91× |
| seoul | rounded | Graph-4096 | 3002.1 | 6203.6 | 2.07× |
| seoul | rounded | Hypergraph-1024 | 4914.0 | 9947.4 | 2.02× |
| seoul | rounded | Hypergraph-256 | 2956.5 | 5872.9 | 1.99× |
| seoul | rounded | Hypergraph-4096 | 3646.6 | 7659.3 | 2.10× |
| seoul | rounded | bidirectional | 226.1 | 401.6 | 1.78× |
| seoul | rounded | dijkstra | 153.3 | 280.5 | 1.83× |
| seoul | rounded | fast_paths-CH | 57433.4 | 113300.6 | 1.97× |
| tokyo-large | native | Graph-1024 | 5550.9 | 11088.2 | 2.00× |
| tokyo-large | native | Graph-256 | 3668.4 | 7299.7 | 1.99× |
| tokyo-large | native | Graph-4096 | 4149.6 | 8183.4 | 1.97× |
| tokyo-large | native | Hypergraph-1024 | 8678.8 | 16957.6 | 1.95× |
| tokyo-large | native | Hypergraph-256 | 6749.8 | 12846.3 | 1.90× |
| tokyo-large | native | Hypergraph-4096 | 5011.4 | 9858.5 | 1.97× |
| tokyo-large | native | bidirectional | 1779.0 | 2840.7 | 1.60× |
| tokyo-large | native | dijkstra | 1176.3 | 2295.8 | 1.95× |
| tokyo-large | rounded | Graph-1024 | 6022.3 | 12090.2 | 2.01× |
| tokyo-large | rounded | Graph-256 | 4118.3 | 7821.1 | 1.90× |
| tokyo-large | rounded | Graph-4096 | 4449.7 | 8669.0 | 1.95× |
| tokyo-large | rounded | Hypergraph-1024 | 9353.7 | 18113.4 | 1.94× |
| tokyo-large | rounded | Hypergraph-256 | 7335.2 | 13953.4 | 1.90× |
| tokyo-large | rounded | Hypergraph-4096 | 5309.8 | 10495.5 | 1.98× |
| tokyo-large | rounded | bidirectional | 1901.2 | 3012.5 | 1.58× |
| tokyo-large | rounded | dijkstra | 1237.3 | 2379.0 | 1.92× |
| tokyo-large | rounded | fast_paths-CH | 94466.0 | 188571.4 | 2.00× |
| ulsan | native | Graph-1024 | 7341.9 | 14576.2 | 1.99× |
| ulsan | native | Graph-256 | 4052.3 | 7929.6 | 1.96× |
| ulsan | native | Graph-4096 | 5038.7 | 9816.0 | 1.95× |
| ulsan | native | Hypergraph-1024 | 11667.1 | 23271.8 | 1.99× |
| ulsan | native | Hypergraph-256 | 7582.1 | 14953.3 | 1.97× |
| ulsan | native | Hypergraph-4096 | 5678.6 | 10993.7 | 1.94× |
| ulsan | native | bidirectional | 537.6 | 1095.5 | 2.04× |
| ulsan | native | dijkstra | 353.4 | 700.6 | 1.98× |
| ulsan | rounded | Graph-1024 | 7258.1 | 14657.5 | 2.02× |
| ulsan | rounded | Graph-256 | 4091.8 | 8047.7 | 1.97× |
| ulsan | rounded | Graph-4096 | 4995.1 | 9887.0 | 1.98× |
| ulsan | rounded | Hypergraph-1024 | 11530.2 | 23346.2 | 2.02× |
| ulsan | rounded | Hypergraph-256 | 7665.8 | 15124.4 | 1.97× |
| ulsan | rounded | Hypergraph-4096 | 5660.5 | 11029.2 | 1.95× |
| ulsan | rounded | bidirectional | 545.8 | 1090.1 | 2.00× |
| ulsan | rounded | dijkstra | 351.0 | 701.2 | 2.00× |
| ulsan | rounded | fast_paths-CH | 87508.3 | 171045.0 | 1.95× |

## Preprocessing

Each overlay total sums median build times across six study configurations for one metric. Deployment builds only its chosen configuration; individual times remain in JSONL. Parallelism covers cell customization; partitioning is serial. CH is one separate rounded-metric build. Ratios are descriptive; regressions below 1 remain visible.
| City | Metric | Overlay study serial s | With parallel customization s | Serial / parallel | CH build s |
|---|---|---:|---:|---:|---:|
| busan | native | 43.18 | 42.46 | 1.017× | n/a |
| busan | rounded | 43.95 | 42.91 | 1.024× | 0.89 |
| daegu | native | 69.17 | 69.08 | 1.001× | n/a |
| daegu | rounded | 68.86 | 68.43 | 1.006× | 1.09 |
| daejeon | native | 29.75 | 29.59 | 1.006× | n/a |
| daejeon | rounded | 28.72 | 28.63 | 1.003× | 0.68 |
| gwangju | native | 44.79 | 44.83 | 0.999× | n/a |
| gwangju | rounded | 46.72 | 45.71 | 1.022× | 0.83 |
| incheon | native | 35.52 | 34.91 | 1.018× | n/a |
| incheon | rounded | 44.94 | 42.74 | 1.051× | 1.12 |
| london-large | native | 12.76 | 12.60 | 1.012× | n/a |
| london-large | rounded | 12.84 | 12.75 | 1.006× | 0.74 |
| new_york-large | native | 18.17 | 18.03 | 1.008× | n/a |
| new_york-large | rounded | 18.18 | 18.03 | 1.008× | 2.23 |
| sejong | native | 7.83 | 7.79 | 1.006× | n/a |
| sejong | rounded | 7.28 | 6.83 | 1.067× | 0.16 |
| seoul | native | 253.17 | 243.45 | 1.040× | n/a |
| seoul | rounded | 205.76 | 200.06 | 1.028× | 2.94 |
| tokyo-large | native | 15.12 | 14.36 | 1.053× | n/a |
| tokyo-large | rounded | 13.24 | 13.00 | 1.019× | 0.81 |
| ulsan | native | 41.29 | 40.44 | 1.021× | n/a |
| ulsan | rounded | 37.39 | 38.29 | 0.977× | 0.63 |

## Correctness population

Each city and weight scale: 1,000 individually certified query distances; 3 certified full trees. Latency and throughput rows report zero wrong answers; overlay build rows checked parity on all 1,000 queries.

## Machine and build provenance

Queries 1,000; repeats 3; threads 2; seed 20261123; git cee8c6b56118351c690f68645f0da89717663d5d; RUSTFLAGS `-C target-cpu=native`.
Comparisons are within each city job. CPU models vary across jobs, so absolute latency differences across cities also reflect different hardware.

| City | CPU | OS | Rust compiler | Logical CPUs | RAM bytes |
|---|---|---|---|---:|---:|
| busan | AMD EPYC 9V74 80-Core Processor | Linux-7.0.0-1020-azure-x86_64-with-glibc2.43 | rustc 1.100.0 (0ef1bc386 2026-11-09)<br>binary: rustc<br>commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a<br>commit-date: 2026-11-09<br>host: x86_64-unknown-linux-gnu<br>release: 1.100.0<br>LLVM version: 23.1.5 | 4 | 16759431168 |
| daegu | AMD EPYC 7763 64-Core Processor | Linux-7.0.0-1020-azure-x86_64-with-glibc2.43 | rustc 1.100.0 (0ef1bc386 2026-11-09)<br>binary: rustc<br>commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a<br>commit-date: 2026-11-09<br>host: x86_64-unknown-linux-gnu<br>release: 1.100.0<br>LLVM version: 23.1.5 | 4 | 16759431168 |
| daejeon | INTEL(R) XEON(R) PLATINUM 8573C | Linux-7.0.0-1020-azure-x86_64-with-glibc2.43 | rustc 1.100.0 (0ef1bc386 2026-11-09)<br>binary: rustc<br>commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a<br>commit-date: 2026-11-09<br>host: x86_64-unknown-linux-gnu<br>release: 1.100.0<br>LLVM version: 23.1.5 | 4 | 16758386688 |
| gwangju | AMD EPYC 9V74 80-Core Processor | Linux-7.0.0-1020-azure-x86_64-with-glibc2.43 | rustc 1.100.0 (0ef1bc386 2026-11-09)<br>binary: rustc<br>commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a<br>commit-date: 2026-11-09<br>host: x86_64-unknown-linux-gnu<br>release: 1.100.0<br>LLVM version: 23.1.5 | 4 | 16759431168 |
| incheon | AMD EPYC 9V45 96-Core Processor | Linux-7.0.0-1020-azure-x86_64-with-glibc2.43 | rustc 1.100.0 (0ef1bc386 2026-11-09)<br>binary: rustc<br>commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a<br>commit-date: 2026-11-09<br>host: x86_64-unknown-linux-gnu<br>release: 1.100.0<br>LLVM version: 23.1.5 | 4 | 16759431168 |
| london-large | INTEL(R) XEON(R) PLATINUM 8573C | Linux-7.0.0-1020-azure-x86_64-with-glibc2.43 | rustc 1.100.0 (0ef1bc386 2026-11-09)<br>binary: rustc<br>commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a<br>commit-date: 2026-11-09<br>host: x86_64-unknown-linux-gnu<br>release: 1.100.0<br>LLVM version: 23.1.5 | 4 | 16758394880 |
| new_york-large | Intel(R) Xeon(R) 6975P-C | Linux-7.0.0-1016-aws-x86_64-with-glibc2.43 | rustc 1.100.0 (0ef1bc386 2026-11-09)<br>binary: rustc<br>commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a<br>commit-date: 2026-11-09<br>host: x86_64-unknown-linux-gnu<br>release: 1.100.0<br>LLVM version: 23.1.5 | 4 | 16463409152 |
| sejong | AMD EPYC 9V45 96-Core Processor | Linux-7.0.0-1020-azure-x86_64-with-glibc2.43 | rustc 1.100.0 (0ef1bc386 2026-11-09)<br>binary: rustc<br>commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a<br>commit-date: 2026-11-09<br>host: x86_64-unknown-linux-gnu<br>release: 1.100.0<br>LLVM version: 23.1.5 | 4 | 16759431168 |
| seoul | AMD EPYC 9V45 96-Core Processor | Linux-7.0.0-1020-azure-x86_64-with-glibc2.43 | rustc 1.100.0 (0ef1bc386 2026-11-09)<br>binary: rustc<br>commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a<br>commit-date: 2026-11-09<br>host: x86_64-unknown-linux-gnu<br>release: 1.100.0<br>LLVM version: 23.1.5 | 4 | 16759431168 |
| tokyo-large | AMD EPYC 9V45 96-Core Processor | Linux-7.0.0-1020-azure-x86_64-with-glibc2.43 | rustc 1.100.0 (0ef1bc386 2026-11-09)<br>binary: rustc<br>commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a<br>commit-date: 2026-11-09<br>host: x86_64-unknown-linux-gnu<br>release: 1.100.0<br>LLVM version: 23.1.5 | 4 | 16759431168 |
| ulsan | AMD EPYC 7763 64-Core Processor | Linux-7.0.0-1020-azure-x86_64-with-glibc2.43 | rustc 1.100.0 (0ef1bc386 2026-11-09)<br>binary: rustc<br>commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a<br>commit-date: 2026-11-09<br>host: x86_64-unknown-linux-gnu<br>release: 1.100.0<br>LLVM version: 23.1.5 | 4 | 16759431168 |
