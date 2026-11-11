#!/usr/bin/env python3
import argparse
import os
import time

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

CHARS = [chr(c) for c in range(32, 127)]


class Net(nn.Module):
    def __init__(self, n=95, c=(24, 48, 96)):
        super().__init__()
        self.c1 = nn.Conv2d(1, c[0], 3, padding=1)
        self.c2 = nn.Conv2d(c[0], c[0], 3, padding=1)
        self.c3 = nn.Conv2d(c[0], c[1], 3, padding=1)
        self.c4 = nn.Conv2d(c[1], c[1], 3, padding=1)
        self.c5 = nn.Conv2d(c[1], c[2], 3, padding=1)
        self.b1, self.b2, self.b3 = nn.BatchNorm2d(c[0]), nn.BatchNorm2d(c[1]), nn.BatchNorm2d(c[2])
        self.fc1 = nn.Linear(c[2] * 4 * 3, 192)
        self.fc2 = nn.Linear(192, n)
        self.drop = nn.Dropout(0.15)

    def forward(self, x):
        x = F.relu(self.c1(x))
        x = F.max_pool2d(F.relu(self.b1(self.c2(x))), 2)
        x = F.relu(self.c3(x))
        x = F.max_pool2d(F.relu(self.b2(self.c4(x))), 2)
        x = F.max_pool2d(F.relu(self.b3(self.c5(x))), 2)
        x = self.drop(x.flatten(1))
        return self.fc2(F.relu(self.fc1(x)))


def load(path):
    d = np.load(path)
    return torch.from_numpy(d["x"]), torch.from_numpy(d["y"])


def evaluate(net, x, y):
    net.eval()
    hit = 0
    wrong = {}
    with torch.no_grad():
        for i in range(0, len(x), 4096):
            xb = x[i:i + 4096].float().div(255).unsqueeze(1)
            p = net(xb).argmax(1)
            yb = y[i:i + 4096]
            hit += (p == yb).sum().item()
            for a, b in zip(yb[p != yb].tolist(), p[p != yb].tolist()):
                wrong[(a, b)] = wrong.get((a, b), 0) + 1
    return hit / len(x), wrong


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", default="/home/claude/glyphdata")
    ap.add_argument("--epochs", type=int, default=8)
    ap.add_argument("--out", default="/home/claude/glyphdata/glyphs.onnx")
    a = ap.parse_args()
    torch.manual_seed(0)
    x, y = load(os.path.join(a.data, "train.npz"))
    vs = load(os.path.join(a.data, "val_seen.npz"))
    vu = load(os.path.join(a.data, "val_unseen.npz"))
    net = Net()
    print(sum(p.numel() for p in net.parameters()), "parameters,", len(x), "samples")
    steps = a.epochs * (len(x) // 256)
    opt = torch.optim.AdamW(net.parameters(), lr=2e-3, weight_decay=1e-4)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=3e-3, total_steps=steps)
    t0 = time.time()
    for ep in range(a.epochs):
        net.train()
        perm = torch.randperm(len(x))
        tot = 0.0
        for i in range(0, len(x) - 255, 256):
            idx = perm[i:i + 256]
            xb = x[idx].float().div(255).unsqueeze(1)
            if ep < a.epochs - 1:
                xb = (xb * (0.85 + 0.3 * torch.rand(len(idx), 1, 1, 1)) + 0.03 * torch.randn_like(xb)).clamp(0, 1)
            loss = F.cross_entropy(net(xb), y[idx], label_smoothing=0.05)
            opt.zero_grad()
            loss.backward()
            opt.step()
            sched.step()
            tot += loss.item()
        s, _ = evaluate(net, *vs)
        u, _ = evaluate(net, *vu)
        print(f"epoch {ep + 1}: loss {tot / (len(x) // 256):.3f}  seen fonts {s:.4f}  unseen fonts {u:.4f}  ({time.time() - t0:.0f} s)", flush=True)
    for name, v in (("seen", vs), ("unseen", vu)):
        acc, wrong = evaluate(net, *v)
        top = sorted(wrong.items(), key=lambda kv: -kv[1])[:30]
        print(name, f"{acc:.4f}", " ".join(f"{CHARS[a]}→{CHARS[b]}:{n}" for (a, b), n in top))
    net.eval()
    torch.save(net.state_dict(), a.out.replace(".onnx", ".pt"))
    torch.onnx.export(net, torch.zeros(1, 1, 32, 24), a.out, input_names=["cells"], output_names=["logits"],
                      dynamic_axes={"cells": {0: "n"}, "logits": {0: "n"}}, opset_version=17, dynamo=False)
    print("saved", a.out, os.path.getsize(a.out))


if __name__ == "__main__":
    main()
