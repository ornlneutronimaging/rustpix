import numpy as np
import rustpix
import time

def generate_large_data(num_hits=1_000_000):
    print(f"Generating {num_hits} hits...")
    # Random hits across full detector 256x256
    x = np.random.randint(0, 256, num_hits).astype(np.uint16)
    y = np.random.randint(0, 256, num_hits).astype(np.uint16)
    # Sorted TOF to mimic real data flow
    tof = np.sort(np.random.randint(0, 1_000_000_000, num_hits).astype(np.uint32))
    tot = np.random.randint(10, 100, num_hits).astype(np.uint16)
    
    hits = []
    # Batch creation if possible? No, bindings take list of objects currently.
    # This might be slow in Python, but we want to benchmark the Rust part.
    # Actually, `read_tpx3_file_numpy` returns dict of arrays.
    # Optimally we should have a `from_numpy` method, but for now we construct PyHit list.
    # To avoid Python loop overhead dominating, we might want to measure only cluster_hits time.
    
    hits = [rustpix.Hit(int(x[i]), int(y[i]), int(tof[i]), int(tot[i]), 0, 0) for i in range(num_hits)]
    return hits

def benchmark():
    # Warmup with small data
    hits_small = generate_large_data(10_000)
    config = rustpix.ClusteringConfig(radius=5.0, temporal_window_ns=200.0, min_cluster_size=1)
    rustpix.cluster_hits(hits_small, config, algorithm="grid")
    
    # 1M hits
    hits_1m = generate_large_data(1_000_000)
    
    algorithms = ["grid", "abs", "dbscan"]
    
    print("\nBenchmarking 1M hits:")
    print("-" * 40)
    print(f"{'Algorithm':<15} | {'Time (ms)':<10} | {'Rate (Mhits/s)':<15}")
    print("-" * 40)
    
    for algo in algorithms:
        print(f"Running {algo}...")
        start = time.time()
        # Run 1 time
        for _ in range(1):
             rustpix.cluster_hits(hits_1m, config, algorithm=algo)
        end = time.time()
        
        avg_time = (end - start) / 1.0
        rate = 1.0 / avg_time # Mhits/s
        
        print(f"{algo:<15} | {avg_time*1000:<10.2f} | {rate:<15.2f}")

if __name__ == "__main__":
    benchmark()
