from mergexp import *

net = Network('two')

def makeNode(name: str):
    return net.node(name, proc.cores>=1, memory.capacity>=mb(512))

sna = [makeNode(name) for name in ['a', 'b']]

link = net.connect(sna)
link[sna[0]].socket.addrs = ip4('10.0.0.1/24')
link[sna[1]].socket.addrs = ip4('10.0.0.2/24')

experiment(net)
