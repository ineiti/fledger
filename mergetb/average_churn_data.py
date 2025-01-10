import numpy as np
import json
import sys
import os
from average_data import metrics_to_extract, get_data, calculate_average_data
import extract_raw_data 

from extract_raw_churn_data import metrics_interval

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: python average_data.py <directory> <duration>")
        sys.exit(1)

    directory = sys.argv[1]
    duration = float(sys.argv[2])
    data = get_data(directory)

    average_data = {}

    print(f"variables: {data.keys()}")

    for variable_name, variable_data in data.items():
    # for variable_name, variable_data in  ["duplicates"]:
    # for variable_name, variable_data in  ["duplicates"]:
    # variable_name = "retry"
    # variable_data = data[variable_name]

        average_data[variable_name] = {}
        print(f"runs: {variable_data.keys()}")
        for run, runs in variable_data.items():
        # run = "4"
            average_data[variable_name][run] = {}
            print("Getting data for: ", variable_name, run)

            for time_index, time_data in variable_data[run].items():
            # for time_index, time_data in runs.items():
                if time_index != "0":
                    prev_step = variable_data[run][str(int(time_index) - metrics_interval)]
                    # print(f"time_index: {time_index}, prev_step: {prev_step}")
                    # print(f"prev_step data: {prev_step['loopix_incoming_messages'], prev_step['loopix_number_of_proxy_requests']}")
                    # print(f"time_data data: {time_data['loopix_incoming_messages'], time_data['loopix_number_of_proxy_requests']}")

                    for metric in extract_raw_data.metrics_to_extract:
                        if metric in ["loopix_bandwidth_bytes", "loopix_incoming_messages", "loopix_number_of_proxy_requests"]:
                            # print(f"{time_index}--------------------------------")
                            prev_step_data = np.array(prev_step[metric])
                            time_data_data = np.array(time_data[metric])
                            # print(f"prev_step_data: {prev_step_data}")
                            # print(f"time_data_data: {time_data_data}")
                            # print(f"data[{metric}]:")
                            if len(prev_step_data) < len(time_data_data):
                                data[metric] = time_data_data[:len(prev_step_data)] - prev_step_data
                            elif len(prev_step_data) > len(time_data_data):
                                data[metric] =  time_data_data - prev_step_data[:len(time_data_data)]
                            else:
                                data[metric] = time_data_data - prev_step_data 
                            # print(f"{data[metric]}")

                        elif metric == "loopix_start_time_seconds":
                            data[metric] = time_data[metric]
                        else:
                            # print("--------------------------------")
                            prev_step_sum = np.array(prev_step[metric]["sum"])
                            time_data_sum = np.array(time_data[metric]["sum"])
                            # print(f"prev_step_sum: {prev_step_sum}")
                            # print(f"time_data_sum: {time_data_sum}")

                            if len(prev_step_sum) < len(time_data_sum):
                                data[metric]["sum"] = time_data_sum[:len(prev_step_sum)] - prev_step_sum
                            elif len(prev_step_sum) > len(time_data_sum):
                                data[metric]["sum"] =  time_data_sum - prev_step_sum[:len(time_data_sum)]
                            else:
                                data[metric]["sum"] = time_data_sum - prev_step_sum 

                            prev_step_count = np.array(prev_step[metric]["count"])
                            time_data_count = np.array(time_data[metric]["count"])
                            # print(f"prev_step_count: {prev_step_count}")
                            # print(f"time_data_count: {time_data_count}")

                            if len(prev_step_count) < len(time_data_count):
                                data[metric]["count"] = time_data_count[:len(prev_step_count)] - prev_step_count
                            elif len(prev_step_count) > len(time_data_count):
                                data[metric]["count"] =  time_data_count - prev_step_count[:len(time_data_count)]
                            else:
                                data[metric]["count"] = time_data_count - prev_step_count 

                            # print(f"data[{metric}]: {data[metric]}")


                else:
                    data = time_data
                    print(f"time_index: {time_index}, data: {data['loopix_incoming_messages'], data['loopix_number_of_proxy_requests']}")

                
                average_data[variable_name][run][time_index] = {}
                print(f"data: {data}")
                # data = time_data
                average_data[variable_name][run][time_index] = calculate_average_data(data, metrics_interval)

    with open(os.path.join(directory, 'average_data.json'), 'w') as f:
        json.dump(average_data, f, indent=2)

   

        
