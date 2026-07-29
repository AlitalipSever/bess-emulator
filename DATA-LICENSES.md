# Dataset licenses

Policy first, inventory second. Every dataset that enters this repository
is recorded here **before** it lands, with its license and redistribution
status. Sources whose terms do not permit redistribution are never bundled;
`bess-data` ships a fetch script instead and this file says so.

## Policy

1. Bundled data must be redistributable under terms compatible with this
   repository's licenses (MIT OR Apache-2.0), with attribution recorded
   here.
2. Restricted sources (for example exchange market data) are fetched by the
   user with a script, from the original source, under the user's own
   acceptance of the source's terms.
3. Each bundled dataset carries a version tag; the determinism contract
   includes the dataset version.

## Inventory

| Dataset | Used for | License / status | Bundled? |
|---|---|---|---|
| Synthetic daily price curve (24 values, hard-coded) | M0 placeholder dispatch plan | Original to this project | yes (code constant) |
| Synthetic weather / frequency driver (sinusoids) | M0 placeholder inputs | Original to this project | yes (code) |
| LFP OCV curve shape (13-point table) | Cell model | Original parameterization informed by public datasheets and published OCV studies | yes (code constant) |

Real historical series (day-ahead prices, weather, grid frequency,
balancing activations) arrive with `bess-data`; each will be added to this
table with its exact source and terms before the first commit that
references it.
