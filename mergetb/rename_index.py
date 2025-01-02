import os
import re
import sys

current_index = sys.argv[1] 
new_index = sys.argv[2]      

directory = "./rename"

files = os.listdir(directory)

pattern = re.compile(rf"^metrics_{current_index}_(node-\d+\.txt)$")

for file in files:
    match = pattern.match(file)
    if match:
        old_file_path = os.path.join(directory, file)
        new_file_name = f"metrics_{new_index}_{match.group(1)}"
        new_file_path = os.path.join(directory, new_file_name)
        
        os.rename(old_file_path, new_file_path)
        print(f"Renamed: {old_file_path} -> {new_file_path}")

    logs_file = os.path.join(directory, f"log_{current_index}_node-1.log")
    signal_file = os.path.join(directory, f"{current_index}_SIGNAL.log")
    config_file = os.path.join(directory, f"{current_index}_config.yaml")
    storage_file = os.path.join(directory, f"{current_index}_storage.yaml")

new_config_file = os.path.join(directory, f"{new_index}_config.yaml")
new_storage_file = os.path.join(directory, f"{new_index}_storage.yaml")
new_logs_file = os.path.join(directory, f"log_{new_index}_node-1.log")
new_signal_file = os.path.join(directory, f"{new_index}_SIGNAL.log")
os.rename(logs_file, new_logs_file)
os.rename(signal_file, new_signal_file)
os.rename(config_file, new_config_file)
os.rename(storage_file, new_storage_file)

print("Renaming complete.")
