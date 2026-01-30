# search-ads-cli

Google Ads API CLI (gRPC, dynamic). Designed for LLM discovery and direct scripting.

## Install

### Install script (macOS arm64 + Linux x86_64)

```bash
curl -fsSL https://raw.githubusercontent.com/radjathaher/search-ads-cli/main/scripts/install.sh | bash
```

### Download from GitHub releases

Grab the latest `search-ads-cli-<version>-<os>-<arch>.tar.gz`, unpack, and move `search-ads` to your PATH.

## Auth

Required:

```bash
export GOOGLE_ADS_DEVELOPER_TOKEN="..."
```

Either set an access token directly:

```bash
export GOOGLE_ADS_ACCESS_TOKEN="ya29..."
```

Or set OAuth refresh creds:

```bash
export GOOGLE_ADS_CLIENT_ID="..."
export GOOGLE_ADS_CLIENT_SECRET="..."
export GOOGLE_ADS_REFRESH_TOKEN="..."
```

Optional:

```bash
export GOOGLE_ADS_LOGIN_CUSTOMER_ID="1234567890"   # manager account
export GOOGLE_ADS_CUSTOMER_ID="1234567890"         # default customer id
export GOOGLE_ADS_ENDPOINT="https://googleads.googleapis.com"
```

## Discovery

```bash
search-ads list --json
search-ads describe google-ads-service search-stream --json
search-ads tree --json
```

## Examples

GAQL search (streamed, aggregate rows):

```bash
search-ads gaql search \
  --customer-id 1234567890 \
  --query 'SELECT campaign.id, campaign.name FROM campaign LIMIT 5' \
  --pretty
```

GAQL search (unary Search):

```bash
search-ads gaql search \
  --customer-id 1234567890 \
  --query 'SELECT campaign.id, campaign.name FROM campaign LIMIT 5' \
  --use-search \
  --page-size 50 \
  --pretty
```

Mutate (generic ops array):

```bash
search-ads mutate \
  --customer-id 1234567890 \
  --ops '[{"campaignOperation":{"create":{"name":"Test","advertisingChannelType":"SEARCH","status":"PAUSED","manualCpc":{}}}}]'
```

Raw call:

```bash
search-ads raw \
  --service google-ads-service \
  --method search-stream \
  --body '{"customerId":"1234567890","query":"SELECT campaign.id FROM campaign LIMIT 1"}'
```

## Regenerate protos + descriptor

```bash
tools/fetch_protos.py --out schemas
tools/build_descriptor.sh
```
