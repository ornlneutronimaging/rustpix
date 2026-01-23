import rustpix
import sys
import time

def benchmark():
    if len(sys.argv) < 2:
        print("Usage: python benchmark.py <input.tpx3>")
        return

    path = sys.argv[1]
    config = rustpix.ClusteringConfig(radius=5.0, temporal_window_ns=200.0, min_cluster_size=1)

    algorithms = ["grid", "abs", "dbscan"]
    
    print(f"\nBenchmarking {path}:")
    print("-" * 40)
    print(f"{'Algorithm':<15} | {'Time (ms)':<10} | {'Rate (Mhits/s)':<15}")
    print("-" * 40)
    
    for algo in algorithms:
        print(f"Running {algo}...")
        start = time.time()
        rustpix.process_tpx3_file(path, config=config, algorithm=algo)
        end = time.time()
        
        avg_time = (end - start) / 1.0
        rate = 1.0 / avg_time # files/s
        
        print(f"{algo:<15} | {avg_time*1000:<10.2f} | {rate:<15.2f}")

if __name__ == "__main__":
    benchmark()
