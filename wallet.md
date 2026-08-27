# X402 Payment Protocol - Agent Guide

## What is x402?

x402 is an open protocol for internet-native payments built on the HTTP 402 status code. It enables you to pay for API resources without registration, emails, OAuth, or complex authentication.

**As a buyer (that's you, the agent):**
1. You request a protected resource
2. Server responds with HTTP 402 Payment Required
3. You create a payment signature using x402-wallet
4. You retry the request with the payment
5. Server validates and returns the resource

The x402-wallet CLI handles step 3 (signing payments). You handle everything else (HTTP requests, parsing responses).

---

## Universal Workflow (Works for Any x402 Resource)

**Note, a list of available resources can be found in the resource-list.md file**

This wallet supports both x402 protocol versions: **v1** (payment header `X-PAYMENT`) and **v2** (payment header `PAYMENT-SIGNATURE`). The server decides which version it speaks — detect it from the 402 response (see Step 2).

### Step 1: Make Initial Request

Make your normal HTTP request to any x402-protected endpoint:

    curl -X POST \
      -H "Content-Type: application/json" \
      -d '{"your":"data"}' \
      https://api.example.com/protected-endpoint

Or for a GET request:

    curl https://api.example.com/protected-resource

### Step 2: Receive 402 Payment Required

Detect the protocol version from the response: **v1** sends payment requirements in the JSON **body**, **v2** sends them Base64-encoded in the **`PAYMENT-REQUIRED` HTTP header** (the body is free for other uses).

**v1 response (requirements in body):**

    {
      "error": "X-PAYMENT header is required",
      "accepts": [
        {
          "scheme": "exact",
          "network": "base",
          "maxAmountRequired": "10000",
          "resource": "https://api.example.com/protected-endpoint",
          "description": "Service description",
          "payTo": "0xRECIPIENT_ADDRESS",
          "asset": "0xTOKEN_CONTRACT_ADDRESS",
          "extra": {
            "name": "USD Coin",
            "version": "2"
          }
        }
      ],
      "x402Version": 1
    }

**v2 response (requirements Base64-encoded in `PAYMENT-REQUIRED` header):**

    {
      "x402Version": 2,
      "error": "PAYMENT-SIGNATURE header is required",
      "resource": {
        "url": "https://api.example.com/protected-endpoint",
        "description": "Service description",
        "mimeType": "application/json"
      },
      "accepts": [
        {
          "scheme": "exact",
          "network": "eip155:8453",
          "amount": "10000",
          "payTo": "0xRECIPIENT_ADDRESS",
          "asset": "0xTOKEN_CONTRACT_ADDRESS",
          "maxTimeoutSeconds": 60,
          "extra": {
            "name": "USDC",
            "version": "2"
          }
        }
      ]
    }

**Key differences between v1 and v2:**

| | v1 | v2 |
|---|---|---|
| Requirements location | JSON response body | `PAYMENT-REQUIRED` header (Base64 JSON) |
| Amount field | `maxAmountRequired` | `amount` |
| Network format | `"base"`, `"ethereum"` | CAIP-2: `"eip155:8453"` |
| Timeout field | — | `maxTimeoutSeconds` |
| Payment header you send | `X-PAYMENT` | `PAYMENT-SIGNATURE` |

**Key fields you need:**
- `payTo` - Recipient wallet address
- `asset` - Token contract address (usually USDC)
- `maxAmountRequired` (v1) / `amount` (v2) - Amount in smallest units (e.g., 10000 = $0.01 for USDC with 6 decimals)
- `network` - Blockchain network (v1: "base"; v2 CAIP-2: "eip155:8453")
- `maxTimeoutSeconds` - Payment validity window; when passed via `--max-timeout-seconds` it bounds the signature window in **both** versions (the echo into the v2 `accepted` object is v2-specific)
- `extra.name` - Token name for signing (e.g., "USD Coin" or "USDC")
- `extra.version` - Token version for signing (e.g., "2")

### Step 3: Extract Payment Parameters

Parse the 402 response and extract the payment details:

    payTo: accepts[0].payTo
    token: accepts[0].asset
    amount: accepts[0].maxAmountRequired   (v1)  /  accepts[0].amount  (v2)
    network: accepts[0].network            (v2 CAIP-2: eip155:1 -> ethereum, eip155:8453 -> base, eip155:84532 -> base-sepolia, eip155:137 -> polygon)
    tokenName: accepts[0].extra.name
    tokenVersion: accepts[0].extra.version
    maxTimeoutSeconds: accepts[0].maxTimeoutSeconds   (bounds the validity window in BOTH versions; v2 requirements always carry it)

### Step 4: Create Payment Signature

Use x402-wallet to create the payment signature. **Always save to a file** to avoid truncation.

**For v1 servers (default):**

    ./target/release/x402-wallet create-payment \
      --pay-to EXTRACTED_PAY_TO_ADDRESS \
      --token EXTRACTED_ASSET_ADDRESS \
      --amount EXTRACTED_AMOUNT \
      --token-name "EXTRACTED_TOKEN_NAME" \
      --token-version "EXTRACTED_TOKEN_VERSION" > payment.txt

**For v2 servers (add `--v2`):**

    ./target/release/x402-wallet create-payment \
      --v2 \
      --resource-url EXTRACTED_RESOURCE_URL \
      --pay-to EXTRACTED_PAY_TO_ADDRESS \
      --token EXTRACTED_ASSET_ADDRESS \
      --amount EXTRACTED_AMOUNT \
      --max-timeout-seconds EXTRACTED_MAX_TIMEOUT_SECONDS \
      --token-name "EXTRACTED_TOKEN_NAME" \
      --token-version "EXTRACTED_TOKEN_VERSION" > payment.txt

**v2 providers with custom extra fields (robust variant, `--accepted`):** some providers (e.g. Exa) add their own fields to `accepts[0]` (`breakdown`, `totalUsd`, `acceptId`). Servers compare the echoed `accepted` object field-by-field (deepEqual), so the complete original object must come back unchanged. In that case pass the FULL `accepts[0]` JSON as one argument:

    ./target/release/x402-wallet create-payment \
      --v2 \
      --pay-to EXTRACTED_PAY_TO_ADDRESS \
      --token EXTRACTED_ASSET_ADDRESS \
      --amount EXTRACTED_AMOUNT \
      --accepted "$(cat accepts0.json)" > payment.txt

(`--accepted` overrides the hand-built echo; the remaining flags are still needed for the EIP-712 signature itself.)

**Example with actual values (v1):**

    ./target/release/x402-wallet create-payment \
      --pay-to 0xB360e5423cB09407B2E5faBf3E656182AbcA6C3A \
      --token 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913 \
      --amount 10000 \
      --token-name "USD Coin" \
      --token-version "2" > payment.txt

**Example with actual values (v2):**

    ./target/release/x402-wallet create-payment \
      --v2 \
      --resource-url https://api.example.com/protected-endpoint \
      --pay-to 0xB360e5423cB09407B2E5faBf3E656182AbcA6C3A \
      --token 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913 \
      --amount 10000 \
      --max-timeout-seconds 60 \
      --token-name "USDC" \
      --token-version "2" > payment.txt

### Step 5: Retry Request with Payment Header

Make the **exact same request** as Step 1, but include the payment header — `X-PAYMENT` for v1 servers, `PAYMENT-SIGNATURE` for v2 servers:

    # v1
    curl -X POST \
      -H "Content-Type: application/json" \
      -H "X-PAYMENT: $(cat payment.txt)" \
      -d '{"your":"data"}' \
      https://api.example.com/protected-endpoint

    # v2
    curl -X POST \
      -H "Content-Type: application/json" \
      -H "PAYMENT-SIGNATURE: $(cat payment.txt)" \
      -d '{"your":"data"}' \
      https://api.example.com/protected-endpoint

### Step 6: Receive Successful Response

The server validates your payment and returns the requested resource:

    HTTP/1.1 200 OK
    {"data": "your requested resource"}

---

## Complete Generic Example

    # 1. Request a protected resource
    curl -X POST \
      -H "Content-Type: application/json" \
      -d '{"query":"analyze this"}' \
      https://api.example.com/analyze

    # Response: 402 Payment Required
    # {"accepts":[{"payTo":"0xABC...","asset":"0x833...","maxAmountRequired":"10000",...}]}

    # 2. Extract payment details from the 402 response
    # payTo: 0xABC...
    # asset: 0x833...
    # amount: 10000
    # extra.name: "USD Coin"
    # extra.version: "2"

    # 3. Create payment signature
    ./target/release/x402-wallet create-payment \
      --pay-to 0xABC... \
      --token 0x833... \
      --amount 10000 \
      --token-name "USD Coin" \
      --token-version "2" > payment.txt

    # 4. Retry with payment
    curl -X POST \
      -H "Content-Type: application/json" \
      -H "X-PAYMENT: $(cat payment.txt)" \
      -d '{"query":"analyze this"}' \
      https://api.example.com/analyze

    # Response: 200 OK
    # {"result": "analysis complete"}

    # 5. Cleanup
    rm payment.txt

---

---

## CLI Path Configuration

The examples in this guide use `./target/release/x402-wallet` which assumes you're running from the repository directory.

**If you've installed the CLI globally or added it to your PATH**, you can simply use:

    x402-wallet create-payment --pay-to ... --token ... --amount ...

Instead of:

    ./target/release/x402-wallet create-payment --pay-to ... --token ... --amount ...

**To add x402-wallet to your PATH:**

Option 1 - Copy to a directory in your PATH:

    sudo cp ./target/release/x402-wallet /usr/local/bin/
    # Now you can use: x402-wallet

Option 2 - Add the target/release directory to your PATH:

    export PATH="$PATH:/path/to/x402-wallet/target/release"
    # Add this line to your ~/.bashrc or ~/.zshrc to make it permanent

Option 3 - Install with cargo:

    cargo install --path .
    # Installs to ~/.cargo/bin/ which is usually in PATH

**For the rest of this guide:**
- Examples use `x402-wallet` (assumes it's in PATH)
- If not in PATH, use `./target/release/x402-wallet` instead
- All functionality is identical regardless of how you invoke the CLI

---

## Understanding the 402 Response Structure

The `accepts` array may contain multiple payment options. Typically you'll use the first one that matches your wallet's configured network.

**Common fields:**

| Field | Description | Example |
|-------|-------------|---------|
| `scheme` | Payment method | `"exact"` (most common) |
| `network` | Blockchain network (v2 uses CAIP-2) | `"base"` / `"eip155:8453"` |
| `maxAmountRequired` / `amount` | Amount in token's smallest unit (v1 / v2 name) | `"10000"` (= $0.01 USDC) |
| `maxTimeoutSeconds` | Payment completion timeout (v2) | `60` |
| `payTo` | Recipient address | `"0xB360e5423cB09407B2E5faBf3E656182AbcA6C3A"` |
| `asset` | Token contract address | `"0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"` |
| `resource` | The protected endpoint | `"https://api.example.com/endpoint"` |
| `description` | What you're paying for | `"API access"` |
| `mimeType` | Response content type | `"application/json"` |
| `extra.name` | Token name for EIP-712 | `"USD Coin"` / `"USDC"` |
| `extra.version` | Token version for EIP-712 | `"2"` |

---

## Network Configuration

Before making payments, ensure your wallet is on the correct network:

**Check current network:**

    cat ~/.x402wallet/config.json

**Set network to Base (most common for x402):**

    ./target/release/x402-wallet config-set --network base

**Set network to Ethereum:**

    ./target/release/x402-wallet config-set --network ethereum

**Set network to Base Sepolia (testnet):**

    ./target/release/x402-wallet config-set --network base-sepolia

**Set network to Polygon:**

    ./target/release/x402-wallet config-set --network polygon

---

## Checking Balance Before Payment

Always verify you have sufficient funds before attempting payment:

**Check USDC balance (Base mainnet):**

    ./target/release/x402-wallet balance --erc20 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913

**Check USDC balance (Base Sepolia testnet):**

    ./target/release/x402-wallet balance --erc20 0x036CbD53842c5426634e7929541eC2318f3dCF7e

**Check ETH balance:**

    ./target/release/x402-wallet balance

**Get your wallet address:**

    ./target/release/x402-wallet wallet-address

---

## Common Token Addresses

**USDC on Base (mainnet):** `0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913`  
**USDC on Base Sepolia:** `0x036CbD53842c5426634e7929541eC2318f3dCF7e`  
**USDC on Ethereum:** `0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48`  
**Native USDC on Polygon:** `0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359`  

⚠️ On Polygon, do NOT use the bridged USDC.e (`0x2791...4174`) — it does not support EIP-3009 and payments will fail. Only native Circle USDC works with this wallet.

---

## Error Handling

### Error: "atob() called with invalid base64"

**Cause:** Payment header was truncated when captured  
**Solution:** Always redirect output to a file:

    x402-wallet create-payment ... > payment.txt
    curl -H "X-PAYMENT: $(cat payment.txt)" ...

### Error: Empty error object `{"error":{},...}`

**Cause:** Payment signature is valid but authorization failed  
**Solutions:**
1. Verify you're on the correct network (check 402 response `network` field)
2. Check you have sufficient balance
3. Ensure payment is fresh (they expire after 10 minutes)

### Error: "RPC chain ID mismatch"

**Cause:** Wallet network doesn't match configured RPC  
**Solution:** Set correct network:

    ./target/release/x402-wallet config-set --network base

### Error: "No private key found"

**Cause:** Wallet not initialized or .env not exported  
**Solution:**

    # If using .env method
    export $(cat .env | xargs)

    # If using keystore method
    # Wallet will prompt for password

---

## Agent Workflow Pattern

When a user asks you to interact with an x402-protected resource:

1. **Make initial request** to the endpoint
2. **Check response status:**
   - If 200 OK → return result to user
   - If 402 Payment Required → continue to step 3
3. **Detect protocol version:** v2 if a `PAYMENT-REQUIRED` header is present, otherwise v1 (requirements in body)
4. **Parse 402 response** to extract: `payTo`, `asset`, `maxAmountRequired` (v1) or `amount` (v2), `maxTimeoutSeconds` (v2), `extra.name`, `extra.version`
5. **Verify network** matches wallet configuration (v2 CAIP-2: `eip155:8453` = base)
6. **Check balance** (optional but recommended)
7. **Create payment signature** using x402-wallet (add `--v2` for v2 servers), save to file
8. **Retry original request** with `X-PAYMENT` (v1) or `PAYMENT-SIGNATURE` (v2) header
9. **Return result** to user
10. **Clean up** temporary payment file

---

## Division of Responsibilities

**YOU (the agent) handle:**
- Making HTTP requests (initial and with payment)
- Parsing 402 responses
- Extracting payment parameters
- Capturing payment header output
- Retrying requests with payment

**x402-wallet CLI handles:**
- Signing EIP-3009 payment authorizations
- Generating X-PAYMENT (v1) / PAYMENT-SIGNATURE (v2) headers
- Managing private keys

---

## Best Practices

### DO ✅

1. **Always save payment to file** - prevents truncation

       x402-wallet create-payment ... > payment.txt

2. **Use command substitution when sending**

       curl -H "X-PAYMENT: $(cat payment.txt)" ...

3. **Parse 402 response fields** - never hardcode payment values

4. **Create fresh payment for each request** - don't reuse signatures

5. **Use same request method and body** - initial request and paid retry must match

6. **Verify network matches** - check 402 `network` field vs wallet config

7. **Check balance before payment** - avoid failed transactions

### DON'T ❌

1. ❌ Copy truncated terminal output manually
2. ❌ Reuse payment signatures (they expire in 10 minutes)
3. ❌ Skip the initial 402 request
4. ❌ Hardcode payment values
5. ❌ Change request body between initial and paid requests
6. ❌ Use keystore mode with agents (blocks on password prompt)

---

## Payment Details

**Protocol:** x402 v1 and v2 (server decides; detect via `PAYMENT-REQUIRED` header)  
**Payment scheme:** "exact" (EIP-3009 transfer with authorization)  
**Token:** USDC (or any ERC20 supporting EIP-3009)  
**Validity:** 10 minutes from signature creation  
**Gas:** Paid by recipient, not sender (gasless for you)  

---

## Multiple x402 Services

This wallet works with **any** x402-protected API. Common use cases:

- AI model APIs
- Data analysis services
- Premium API endpoints
- Computational resources
- Content generation services
- Storage services
- Real-time data feeds

The workflow is always the same:
1. Request → 402 response
2. Parse payment details
3. Create signature
4. Retry with payment

---

## Example: Different Service Types

**Data API (GET request):**

    # 1. Initial request
    curl https://data-api.example.com/dataset/123

    # 2. Got 402, create payment
    x402-wallet create-payment --pay-to 0xABC... --token 0x833... --amount 5000 > payment.txt

    # 3. Retry with payment
    curl -H "X-PAYMENT: $(cat payment.txt)" https://data-api.example.com/dataset/123

**AI Service (POST request with JSON):**

    # 1. Initial request
    curl -X POST -H "Content-Type: application/json" \
      -d '{"prompt":"generate image"}' \
      https://ai-api.example.com/generate

    # 2. Got 402, create payment
    x402-wallet create-payment --pay-to 0xDEF... --token 0x833... --amount 25000 > payment.txt

    # 3. Retry with payment
    curl -X POST -H "Content-Type: application/json" \
      -H "X-PAYMENT: $(cat payment.txt)" \
      -d '{"prompt":"generate image"}' \
      https://ai-api.example.com/generate

**File Storage (PUT request):**

    # 1. Initial request
    curl -X PUT --data-binary @file.jpg https://storage.example.com/upload

    # 2. Got 402, create payment
    x402-wallet create-payment --pay-to 0xGHI... --token 0x833... --amount 15000 > payment.txt

    # 3. Retry with payment
    curl -X PUT -H "X-PAYMENT: $(cat payment.txt)" \
      --data-binary @file.jpg https://storage.example.com/upload

---

## Key Takeaways

1. **x402 is universal** - This workflow works for ANY x402-protected resource
2. **Detect the version** - `PAYMENT-REQUIRED` header present = v2, otherwise v1; `--v2` flag + `PAYMENT-SIGNATURE` header for v2
3. **Wallet only signs** - You handle all HTTP communication
4. **Always parse 402 response** - Payment details vary by service
5. **Save to file** - Prevents truncation issues
6. **10-minute validity** - Create fresh payments for each request
7. **Network matters** - Ensure wallet is on correct network (v2 uses CAIP-2 ids)
8. **Same request twice** - Initial request and paid retry must be identical

---

## Links

**x402 Protocol:** https://x402.org  
**x402 Documentation:** https://x402.gitbook.io/x402  
**x402 v1-to-v2 Migration Guide:** https://docs.x402.org/guides/migration-v1-to-v2  
**EIP-3009 Specification:** https://eips.ethereum.org/EIPS/eip-3009  
**Wallet Repository:** https://github.com/wurminator/x402-wallet-v2  
**x402 Ecosystem:** https://x402.gitbook.io/x402/getting-started/ecosystem