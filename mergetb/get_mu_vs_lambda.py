import numpy as np

initial_chaff = 2
initial_lambda_payload = 4

n_client = 3
n_mixnodes = 9
n_providers = 3
mixnodes_per_layer = 3
initial_mu = 10
    
total_messages_per_second = n_client*(initial_lambda_payload + initial_chaff*2) + (n_mixnodes + n_providers)*(initial_chaff*2)

print(f"Total messages per second for mu {initial_mu}: {total_messages_per_second}")

messages_per_mixnode_per_second = total_messages_per_second/(mixnodes_per_layer)

print(f"Average messages per second at one mixnode: {messages_per_mixnode_per_second}")

print(f"Average messages at one node over 5 minutes: {messages_per_mixnode_per_second*300}")

mean_delays=[50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200]
mu_values = [20, 16.67, 14.29, 12.5, 11.11, 10, 9.09, 8.33, 7.69, 7.14, 6.67, 6.25, 5.88, 5.56, 5.26, 5]

target_length = len(mean_delays)

target_lambda_over_mu = messages_per_mixnode_per_second/initial_mu

print(f"Target lambda over mu: {target_lambda_over_mu}")

lambda_values = [mu_values[i]*target_lambda_over_mu for i in range(len(mu_values))]

print(f"Lambda values: {lambda_values}")

mu_ratio_to_reference = [mu/initial_mu for mu in mu_values]
print(f"Mu ratios to reference: {mu_ratio_to_reference}")

target_total_messages_per_second = [total_messages_per_second*mu for mu in mu_ratio_to_reference]
print(f"Target total messages per second: {target_total_messages_per_second}")

payload_values = np.linspace(8, 4, 6)
payload_values = np.append(payload_values[:-1], np.linspace(4, 2.5, target_length-len(payload_values)))

print(f"Payload values: {payload_values}")
chaff_values =   [4, 3.3, 2.79, 2.45, 2.2, 2, 1.79, 1.62, 1.48, 1.37, 1.27, 1.18, 1.11, 1.05, 0.99, 0.95]

total_messages_per_second = [n_client*(payload_values[i] + chaff_values[i]*2) + (n_mixnodes + n_providers)*(chaff_values[i]*2) for i, payload in enumerate(payload_values)]
print(f"Total messages per second: {total_messages_per_second}")

for target_total_messages, total_messages in zip(target_total_messages_per_second, total_messages_per_second):
    print(f"Target messages: {target_total_messages}, Total messages: {total_messages}")


print("for same mu (10):")
lambda_payloads=[2, 2.25, 2.5, 2.75, 3, 3.25, 3.5, 3.75, 4, 4.25, 4.5, 4.75, 5, 5.25, 5.5, 5.75, 6, 6.25, 6.5, 6.75, 7, 7.25, 7.5, 7.75, 8, 8.25, 8.5, 8.75, 9, 9.25, 9.5, 9.75, 10]
chaff_lambdas=[2.2, 2.175, 2.15, 2.125, 2.1, 2.075, 2.05, 2.025, 2, 1.975, 1.95, 1.925, 1.9, 1.875, 1.85, 1.825, 1.8, 1.775, 1.75, 1.725, 1.7,1.675, 1.65, 1.625, 1.6, 1.575, 1.55, 1.525, 1.5, 1.475, 1.45, 1.425, 1.4, 1.375, 1.35, 1.325]

total_messages_per_second = [n_client*(lambda_payloads[i] + chaff_lambdas[i]*2) + (n_mixnodes + n_providers)*(chaff_lambdas[i]*2) for i, lambda_payload in enumerate(lambda_payloads)]
for total_messages in total_messages_per_second:
    print(f"Target messages: {72}, Total messages: {total_messages}")


