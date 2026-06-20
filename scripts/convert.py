#!/usr/bin/env python3
from PIL import Image
import sys

# Get image path from command line argument
if len(sys.argv) < 2:
    print("Usage: python convert.py <image_path> [output_path]")
    sys.exit(1)

img = Image.open(sys.argv[1])

# Resize to display dimensions first
img = img.resize((800, 480), Image.Resampling.LANCZOS)

# Convert to grayscale first
img = img.convert('L')

# Convert to pure black and white (1-bit) with dithering
img = img.convert('1')

# invert 1 bit image colors (optional, depending on display requirements)
img = Image.eval(img, lambda x: 255 - x)

# Get output path from command line argument (or use default)
output_path = sys.argv[2] if len(sys.argv) > 2 else 'output.bmp'

# Save as 1-bit BMP (monochrome)
img.save(output_path, 'BMP')

print(f"Converted to 1-bit black and white BMP: {output_path}")
print(f"Image mode: {img.mode}, Size: {img.size}")

# Calculate file size
import os
file_size = os.path.getsize(output_path)