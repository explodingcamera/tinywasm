import re
import sys
from collections import Counter

seq_len = 5

# Check if a file path was provided
if len(sys.argv) < 2:
    print("Usage: python script.py path/to/yourfile.wat")
    sys.exit(1)

# The first command line argument is the file path
file_path = sys.argv[1]

# Regex to match WASM operators, adjust as necessary
operator_pattern = re.compile(r"\b[a-z0-9_]+\.[a-z0-9_]+\b")

# Read the file
with open(file_path, "r") as file:
    content = file.read()

# Find all operators
operators = operator_pattern.findall(content)

# Generate sequences of three consecutive operators
sequences = [" ".join(operators[i : i + seq_len]) for i in range(len(operators) - 2)]

# Count occurrences of each sequence
sequence_counts = Counter(sequences)

# Sort sequences by their count, this time in ascending order for reverse display
sorted_sequences = sorted(sequence_counts.items(), key=lambda x: x[1])

# Print the sequences, now from least common to most common
for sequence, count in sorted_sequences:
    print(f"{sequence}: {count}")
