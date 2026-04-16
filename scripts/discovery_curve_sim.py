#!/usr/bin/env python3
"""
Discovery Multiplier Curve Comparison

Compares candidate curves for the AMOS Grand Challenge discovery multiplier.
All curves map from FLOOR (150%) to CEILING (300%) over ~10 years.

This is a focused simulation for curve selection. The comprehensive
parameter optimization across ALL system curves (emission, decay, growth,
discovery) is RESEARCH-001.
"""

import math
import json

# Parameters
FLOOR = 15000  # 150% in BPS
CEILING = 30000  # 300% in BPS
RANGE = CEILING - FLOOR
MIDPOINT_DAYS = 1825  # 5 years
MAX_DAYS = 3650 + 365  # ~11 years for visualization

# Sample points (monthly for 11 years)
days = list(range(0, MAX_DAYS + 1, 30))
years = [d / 365.25 for d in days]


# ─── Curve Definitions ───────────────────────────────────────────────

def logistic_sigmoid(t, k=0.005, midpoint=MIDPOINT_DAYS):
    """Standard logistic sigmoid (current implementation). Symmetric S-curve."""
    x = -k * (t - midpoint)
    sig = 1.0 / (1.0 + math.exp(x))
    return FLOOR + RANGE * sig


def gompertz(t, a=6.0, b=0.0015):
    """Gompertz curve. Asymmetric: faster initial ramp, slower ceiling approach."""
    # Gompertz: y = ceiling * exp(-a * exp(-b*t))
    # Normalized to [FLOOR, CEILING]
    g = math.exp(-a * math.exp(-b * t))
    return FLOOR + RANGE * g


def log_logistic(t, alpha=1825, beta=3.0):
    """Log-logistic (Fisk distribution CDF). Heavier tails than logistic."""
    if t <= 0:
        return FLOOR
    x = t / alpha
    ll = (x ** beta) / (1.0 + x ** beta)
    return FLOOR + RANGE * ll


def power_curve(t, T=3650, p=0.5):
    """Power curve. p<1 = fast early (sqrt), p>1 = slow early (quadratic)."""
    ratio = min(t / T, 1.0)
    return FLOOR + RANGE * (ratio ** p)


def tanh_curve(t, k=0.003, midpoint=MIDPOINT_DAYS):
    """Hyperbolic tangent. Similar to sigmoid but parameterized differently."""
    x = k * (t - midpoint)
    th = (math.tanh(x) + 1.0) / 2.0
    return FLOOR + RANGE * th


def linear_ramp(t, T=3650):
    """Linear ramp with hard cap. Simple, predictable, no inflection."""
    ratio = min(t / T, 1.0)
    return FLOOR + RANGE * ratio


# ─── Curve Variants ──────────────────────────────────────────────────

curves = {
    # Current implementation
    "Logistic (k=0.005, current)": lambda t: logistic_sigmoid(t, k=0.005),

    # Steeper logistic — faster transition in years 3-7
    "Logistic (k=0.008, steeper)": lambda t: logistic_sigmoid(t, k=0.008),

    # Gentler logistic — more gradual
    "Logistic (k=0.003, gentler)": lambda t: logistic_sigmoid(t, k=0.003),

    # Gompertz — asymmetric, front-loaded
    "Gompertz (a=6, b=0.0015)": lambda t: gompertz(t, a=6.0, b=0.0015),

    # Gompertz — more aggressive early ramp
    "Gompertz (a=4, b=0.001)": lambda t: gompertz(t, a=4.0, b=0.001),

    # Log-logistic — heavier tails
    "Log-logistic (beta=3)": lambda t: log_logistic(t, alpha=1825, beta=3.0),

    # Power curves
    "Power (sqrt, p=0.5)": lambda t: power_curve(t, p=0.5),
    "Power (cube root, p=0.33)": lambda t: power_curve(t, p=0.33),

    # Tanh
    "Tanh (k=0.003)": lambda t: tanh_curve(t, k=0.003),

    # Linear baseline
    "Linear ramp": lambda t: linear_ramp(t),
}


# ─── Evaluation Metrics ──────────────────────────────────────────────

def evaluate_curve(name, fn):
    """Compute key metrics for a curve."""
    values = [fn(d) for d in days]

    # Key milestone values (in %)
    y1 = fn(365) / 100
    y2 = fn(730) / 100
    y3 = fn(1095) / 100
    y5 = fn(1825) / 100
    y7 = fn(2555) / 100
    y10 = fn(3650) / 100

    # Time to reach 200% (midpoint of range)
    target_200 = 20000
    days_to_200 = None
    for d in range(0, MAX_DAYS):
        if fn(d) >= target_200:
            days_to_200 = d
            break

    # Time to reach 250%
    target_250 = 25000
    days_to_250 = None
    for d in range(0, MAX_DAYS):
        if fn(d) >= target_250:
            days_to_250 = d
            break

    # Time to reach 280% (near ceiling)
    target_280 = 28000
    days_to_280 = None
    for d in range(0, MAX_DAYS):
        if fn(d) >= target_280:
            days_to_280 = d
            break

    # "Early incentive" — average multiplier in first 2 years
    early_avg = sum(fn(d) for d in range(0, 730, 30)) / len(range(0, 730, 30)) / 100

    # "Mature incentive" — average multiplier in years 5-10
    mature_avg = sum(fn(d) for d in range(1825, 3650, 30)) / len(range(1825, 3650, 30)) / 100

    # Monotonicity check
    prev = fn(0)
    monotonic = True
    for d in range(30, MAX_DAYS, 30):
        v = fn(d)
        if v < prev - 0.001:
            monotonic = False
            break
        prev = v

    # Bounds check
    in_bounds = all(FLOOR <= fn(d) <= CEILING for d in range(0, MAX_DAYS))

    return {
        "name": name,
        "y1": f"{y1:.1f}%",
        "y2": f"{y2:.1f}%",
        "y3": f"{y3:.1f}%",
        "y5": f"{y5:.1f}%",
        "y7": f"{y7:.1f}%",
        "y10": f"{y10:.1f}%",
        "days_to_200": days_to_200,
        "days_to_250": days_to_250,
        "days_to_280": days_to_280,
        "early_avg": f"{early_avg:.1f}%",
        "mature_avg": f"{mature_avg:.1f}%",
        "monotonic": monotonic,
        "in_bounds": in_bounds,
    }


# ─── Run Simulation ──────────────────────────────────────────────────

print("=" * 90)
print("AMOS Grand Challenge — Discovery Multiplier Curve Comparison")
print("=" * 90)
print(f"\nFloor: {FLOOR/100:.0f}%  |  Ceiling: {CEILING/100:.0f}%  |  Midpoint: {MIDPOINT_DAYS} days (~5 years)")
print()

results = []
for name, fn in curves.items():
    results.append(evaluate_curve(name, fn))

# Print trajectory table
print("-" * 90)
print(f"{'Curve':<35} {'Y1':>6} {'Y2':>6} {'Y3':>6} {'Y5':>6} {'Y7':>6} {'Y10':>6}")
print("-" * 90)
for r in results:
    print(f"{r['name']:<35} {r['y1']:>6} {r['y2']:>6} {r['y3']:>6} {r['y5']:>6} {r['y7']:>6} {r['y10']:>6}")

# Print milestone table
print()
print("-" * 90)
print(f"{'Curve':<35} {'→200%':>8} {'→250%':>8} {'→280%':>8} {'Early':>7} {'Mature':>8} {'OK':>4}")
print(f"{'':35} {'(days)':>8} {'(days)':>8} {'(days)':>8} {'avg':>7} {'avg':>8} {'':>4}")
print("-" * 90)
for r in results:
    d200 = f"{r['days_to_200']:>5}d" if r['days_to_200'] else "  never"
    d250 = f"{r['days_to_250']:>5}d" if r['days_to_250'] else "  never"
    d280 = f"{r['days_to_280']:>5}d" if r['days_to_280'] else "  never"
    ok = "Y" if r['monotonic'] and r['in_bounds'] else "N"
    print(f"{r['name']:<35} {d200:>8} {d250:>8} {d280:>8} {r['early_avg']:>7} {r['mature_avg']:>8} {ok:>4}")

# Print ASCII chart for top candidates
print()
print("=" * 90)
print("ASCII Trajectory (% multiplier over time)")
print("=" * 90)

chart_curves = [
    ("Logistic (k=0.005, current)", curves["Logistic (k=0.005, current)"]),
    ("Gompertz (a=6, b=0.0015)", curves["Gompertz (a=6, b=0.0015)"]),
    ("Log-logistic (beta=3)", curves["Log-logistic (beta=3)"]),
    ("Power (sqrt, p=0.5)", curves["Power (sqrt, p=0.5)"]),
]

symbols = ["*", "G", "L", "P"]
chart_width = 60
chart_height = 20

for idx, (name, fn) in enumerate(chart_curves):
    print(f"  {symbols[idx]} = {name}")

print()
print(f"  300% |")

# Build chart grid
grid = [[" " for _ in range(chart_width)] for _ in range(chart_height)]

for idx, (name, fn) in enumerate(chart_curves):
    for col in range(chart_width):
        day = int(col * MAX_DAYS / chart_width)
        val = fn(day)
        pct = (val - FLOOR) / RANGE
        row = chart_height - 1 - int(pct * (chart_height - 1))
        row = max(0, min(chart_height - 1, row))
        if grid[row][col] == " " or grid[row][col] == ".":
            grid[row][col] = symbols[idx]
        elif grid[row][col] != symbols[idx]:
            grid[row][col] = "+"  # overlap

for row_idx, row in enumerate(grid):
    if row_idx == 0:
        label = "300%"
    elif row_idx == chart_height // 2:
        label = "225%"
    elif row_idx == chart_height - 1:
        label = "150%"
    else:
        label = "    "
    print(f"  {label} |{''.join(row)}|")

# X-axis
print(f"       +{'-' * chart_width}+")
print(f"       0    1    2    3    4    5    6    7    8    9   10  years")

# Analysis
print()
print("=" * 90)
print("ANALYSIS")
print("=" * 90)
print("""
Key tradeoffs:

1. LOGISTIC SIGMOID (current, k=0.005)
   + Symmetric, well-understood, matches emission curve philosophy
   + Barely moves years 1-2 (platform stabilization period)
   - Slow early ramp may under-incentivize early discovery work
   - Most movement concentrated in years 3-7

2. GOMPERTZ (recommended alternative)
   + Asymmetric: faster early ramp than logistic
   + Gets to 200% sooner — earlier discovery incentives
   + Slower approach to ceiling — more runway for growth
   - Less standard, harder to explain to community

3. LOG-LOGISTIC
   + Similar shape to logistic but with heavier tails
   + Slightly faster early ramp
   - Not significantly different from logistic for this use case

4. POWER (sqrt)
   + Fastest early ramp — immediately starts climbing
   + Simple, predictable, no inflection point
   - No S-curve behavior — constant deceleration
   - May incentivize "too early" before platform is ready

RECOMMENDATION:
The current logistic sigmoid (k=0.005) is reasonable. If early discovery
incentives matter, consider either:
  a) Steeper logistic (k=0.008) — same shape, faster transition
  b) Gompertz — asymmetric, front-loads the growth more

This is a focused comparison. The comprehensive multi-curve optimization
(emission × decay × growth × discovery interactions) is RESEARCH-001.
""")

# Output JSON for potential further analysis
output = {
    "parameters": {
        "floor_bps": FLOOR,
        "ceiling_bps": CEILING,
        "midpoint_days": MIDPOINT_DAYS,
    },
    "curves": {}
}
for name, fn in curves.items():
    output["curves"][name] = {
        "trajectory": {str(d): round(fn(d), 1) for d in range(0, MAX_DAYS + 1, 365)},
        "metrics": next(r for r in results if r["name"] == name),
    }

with open("/tmp/discovery_curve_comparison.json", "w") as f:
    json.dump(output, f, indent=2, default=str)
    print("Detailed data written to /tmp/discovery_curve_comparison.json")
