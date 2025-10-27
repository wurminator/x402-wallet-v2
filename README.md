![x402-wallet banner](x402-wallet.png)


# x402-wallet

A command-line wallet for the [x402 payment protocol](https://x402.org). Create cryptographic payment authorizations for pay-per-use APIs that use the x402 payment protocol.

## Features

- ✅ **X402 payments** - Create EIP-3009 payment signatures for x402-protected APIs and resources
- ✅ **Multi-network** - Supports Ethereum, Base, and Base Sepolia
- ✅ **Secure storage** - Encrypted keystore or `.env` file
- ✅ **Full wallet** - Check balances, send ETH/ERC20 tokens
- ✅ **LLM-friendly** - Designed for use by AI coding agents (Claude Code, Gemini etc)

--- 

⚠️ **ACTIVE DEVELOPMENT - USE AT YOUR OWN RISK**

This software is under active development and may contain bugs or experience breaking changes. **No warranty is provided.** You are solely responsible for the security of your private keys and any loss of funds.

**Security Warnings:**
- Private keys are stored locally on your machine (either in `.env` or encrypted keystore)
- If your machine is compromised, your funds may be stolen
- Always use a dedicated wallet with **minimal funds** for testing
- **Never** store large amounts of crypto in CLI wallets
- Review the code yourself before trusting it with real funds
- The EIP-3009 payment signatures authorize token transfers - treat them like signed checks

**Recommendations:**
- ✅ Use a fresh wallet with only test amounts ($1-10 USD)
- ✅ Review source code before running
- ✅ Test on Base Sepolia testnet first
- ❌ Do NOT use your main wallet or seed phrase
- ❌ Do NOT store significant funds in this wallet

**If you are unsure about the security implications, do not use this software.**

---

## Quick Start

### Installation

    git clone https://github.com/0xkoda/x402-wallet
    cd x402-wallet
    cargo build --release

The binary will be at `./target/release/x402-wallet`

### Setup

**IMPORTANT: Choose your storage method**

You have two options for storing your private key:

#### Option A: `.env` file (Recommended for Automation/AI Agents)

**Pros:**
- No password prompts
- Works seamlessly with AI coding agents (Claude Code, Gemini Code Assist, etc.)
- Simple and fast

**Cons:**
- Private key stored in plaintext (protected by file permissions only)
- If someone gains access to your `.env` file, they have your key

**How to use:**

    # Initialize wallet (creates .env file)
    ./target/release/x402-wallet wallet-init

At the prompt `Create new private key (y/N)?`:
- Press `y` to generate a new random private key
- Press `N` to import your own existing private key (you'll be prompted to paste it)

Then export the private key to your shell:

    # Export the private key to your shell environment
    export $(cat .env | xargs)

    # Now all commands work without password prompts
    ./target/release/x402-wallet wallet-address

**When to use:**
- ✅ AI agent automation (Claude Code, Gemini, etc.)
- ✅ Scripts and programmatic access
- ✅ Testing and development
- ✅ Wallets with minimal funds only

#### Option B: Encrypted keystore (More Secure, Manual Use Only)

**Pros:**
- Private key encrypted with your password (XChaCha20-Poly1305 + Argon2)
- Safer if your filesystem is compromised

**Cons:**
- **Blocks automation** - prompts for password on every command
- **Breaks AI agent workflows** - agents cannot enter passwords interactively
- Slower (password derivation takes ~100ms)

**How to use:**

    # Initialize with encrypted keystore
    ./target/release/x402-wallet wallet-init --keystore

At the prompt `Create new private key (y/N)?`:
- Press `y` to generate a new random private key
- Press `N` to import your own existing private key (you'll be prompted to paste it)

Set a strong passphrase when prompted. Every command will then ask for your password:

    # Every command will ask for password
    ./target/release/x402-wallet wallet-address
    # (prompts: "Unlock keystore passphrase:")

**When to use:**
- ✅ Manual/interactive use only
- ✅ When you want password-protected keys
- ❌ **NOT for AI agents** - they cannot enter passwords
- ❌ **NOT for automation** - scripts will hang on password prompt

### Fund Your Wallet

**Option 1: Generate a new wallet and fund it**

1. Initialize wallet (see Setup above)

2. Get your wallet address:

       ./target/release/x402-wallet wallet-address

3. Send USDC to that address on Base mainnet (use a small amount for testing, $1-10 recommended)

4. Check your balance:

       ./target/release/x402-wallet balance --erc20 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913

**Option 2: Import an existing wallet with funds**

1. Run wallet init:

       ./target/release/x402-wallet wallet-init

2. When prompted `Create new private key (y/N)?`, press `N`

3. Paste your existing private key when prompted (format: `0x...` or just the hex string)

4. The wallet will use your existing address and funds

5. Verify it worked:

       ./target/release/x402-wallet wallet-address
       ./target/release/x402-wallet balance --erc20 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913

**IMPORTANT:** Only import a private key for a wallet with minimal funds. Never use your main wallet.

## Usage

### X402 Payments

Pay for an x402-protected API:

    # 1. Make initial request to get payment requirements
    curl -X POST https://api.example.com/endpoint

    # Response: 402 Payment Required with details

    # 2. Create payment signature (save to file)
    ./target/release/x402-wallet create-payment \
      --pay-to 0xRECIPIENT_ADDRESS \
      --token 0xUSDC_ADDRESS \
      --amount 10000 \
      --token-name "USD Coin" \
      --token-version "2" > payment.txt

    # 3. Retry request with payment header
    curl -X POST \
      -H "X-PAYMENT: $(cat payment.txt)" \
      https://api.example.com/endpoint

### Wallet Operations

**Check balances:**

    # ETH balance
    ./target/release/x402-wallet balance

    # USDC balance (Base mainnet)
    ./target/release/x402-wallet balance --erc20 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913

**Send ETH:**

    ./target/release/x402-wallet send-eth \
      --to 0xRECIPIENT \
      --eth 0.1

**Send USDC:**

    ./target/release/x402-wallet send-erc20 \
      --token 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913 \
      --to 0xRECIPIENT \
      --amount 10.5

## Architecture

This wallet implements the **"exact"** payment scheme from x402 using **EIP-3009** (gasless USDC transfers). It creates cryptographic signatures that authorize token transfers without requiring an on-chain transaction from the payer.

**Key components:**
- `create-payment` command - Signs EIP-712 typed data for EIP-3009 authorization
- Payment validity: 10 minutes from creation
- Output: Base64-encoded `X-PAYMENT` header

## Configuration

Config stored in `~/.x402wallet/config.json`:

    {
      "network": "base",
      "rpc": {
        "ethereum": "https://cloudflare-eth.com",
        "base": "https://mainnet.base.org",
        "base-sepolia": "https://sepolia.base.org"
      }
    }

Change networks:

    ./target/release/x402-wallet config-set --network base

## Security

### Private Key Storage

**`.env` file method:**
- Stored in plaintext at `./.env`
- File permissions set to `0600` (owner read/write only) on Unix
- Vulnerable if attacker gains filesystem access
- Environment variable: `X402_WALLET_PRIVATE_KEY`

**Keystore method:**
- Stored encrypted at `~/.x402wallet/keystore.json`
- Encryption: XChaCha20-Poly1305 authenticated encryption
- Key derivation: Argon2 (memory-hard, resistant to GPU attacks)
- Only decrypted in memory when needed, then zeroized
- Still vulnerable if attacker has keylogger or memory access

### Payment Security

- **Payments are time-limited** - expire after 10 minutes
- **Payments are stateless** - no funds locked or at risk if header is intercepted
- **No gas fees for payer** - recipient executes the transfer using your signature
- **Once signed, cannot be revoked** - treat like a signed check

### Best Practices

1. **Use minimal funds** - Only keep what you need for immediate payments
2. **Separate wallets** - Don't reuse wallets from other applications
3. **Test on testnets first** - Use Base Sepolia before mainnet
4. **Review transactions** - Understand what you're signing
5. **Keep software updated** - Pull latest changes regularly
6. **Secure your machine** - Use full-disk encryption, strong passwords, etc.

## Supported Networks

| Network | Chain ID | Default RPC |
|---------|----------|-------------|
| Ethereum | 1 | `https://cloudflare-eth.com` |
| Base | 8453 | `https://mainnet.base.org` |
| Base Sepolia | 84532 | `https://sepolia.base.org` |

## Use with AI Agents

This wallet is designed for use by LLM coding agents (Claude Code, Gemini Code Assist, etc.).

**IMPORTANT:** AI agents **cannot use the encrypted keystore** because they cannot enter passwords interactively. Always use the `.env` method for agent workflows.

Setup for agents:

    # 1. Initialize with .env (not --keystore)
    ./target/release/x402-wallet wallet-init

    # 2. Export the key
    export $(cat .env | xargs)

    # 3. Now agents can use all commands
    ./target/release/x402-wallet create-payment ...

See `res.md` for detailed agent instructions.

**Example workflow:**
1. Agent makes HTTP request → gets 402 Payment Required
2. Agent parses payment details from response
3. Agent calls `x402-wallet create-payment` with parsed params
4. Agent retries HTTP request with `X-PAYMENT` header

## Token Addresses

**USDC on Base (mainnet):** `0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913`  
**USDC on Base Sepolia:** `0x036CbD53842c5426634e7929541eC2318f3dCF7e`  
**USDC on Ethereum:** `0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48`

## Commands Reference

    wallet-init               Initialize new wallet
      --keystore              Use encrypted keystore (breaks automation)
      --dotenv PATH           Use .env file at PATH (default: ./.env)

    wallet-address            Display wallet address

    config-set                Configure network
      --network NAME          Network: ethereum, base, base-sepolia
      --rpc URL               Custom RPC endpoint (optional)

    balance                   Check balance
      --erc20 ADDRESS         Token address (omit for ETH)

    send-eth                  Send ETH
      --to ADDRESS            Recipient
      --eth AMOUNT            Amount in ETH

    send-erc20                Send ERC20 tokens
      --token ADDRESS         Token contract
      --to ADDRESS            Recipient
      --amount AMOUNT         Amount in token units

    create-payment            Create x402 payment signature
      --pay-to ADDRESS        Recipient (from 402 response)
      --token ADDRESS         Token contract (from 402 response)
      --amount UNITS          Amount in smallest units (from 402 response)
      --token-name NAME       Token name for EIP-712 (optional)
      --token-version VER     Token version for EIP-712 (optional)

## Troubleshooting

**"RPC chain ID mismatch"**  
Your configured network doesn't match the RPC endpoint. Set correct network:

    ./target/release/x402-wallet config-set --network base

**"No private key found"**  
Initialize wallet first:

    ./target/release/x402-wallet wallet-init

**"Unlock keystore passphrase:" prompt blocks my script/agent**  
You're using encrypted keystore mode. For automation, use `.env` method instead:

    # Remove keystore
    rm ~/.x402wallet/keystore.json
    
    # Reinitialize with .env
    ./target/release/x402-wallet wallet-init
    export $(cat .env | xargs)

**"Transaction dropped"**  
Insufficient ETH for gas or network congestion. Check ETH balance:

    ./target/release/x402-wallet balance

**Payment authorization fails**  
Ensure you're using correct `--token-name` and `--token-version` from the 402 response `extra` fields.

## Development

Build from source:

    cargo build --release

Run tests:

    cargo test

Format code:

    cargo fmt

Lint:

    cargo clippy

## How It Works

1. **You request a protected resource** → Server responds 402 Payment Required
2. **Server tells you payment details** → `payTo`, `token`, `amount`, etc.
3. **You sign an authorization** → EIP-3009 `TransferWithAuthorization`
4. **You retry with signed payment** → Server validates and processes request
5. **Server executes token transfer** → Uses your signature to claim payment

The beauty of EIP-3009: **You don't pay gas fees**. The recipient executes the transfer using your signed authorization.

## Contributing

Contributions welcome! Please:
1. Fork the repository
2. Create a feature branch
3. Make your changes with tests
4. Submit a pull request

By contributing, you agree to license your contributions under AGPL-3.0.

## License

**AGPL-3.0** - This software is licensed under the GNU Affero General Public License v3.0.

**What this means:**
- ✅ Free for personal use, research, and open-source projects
- ✅ You can modify and redistribute the code
- ⚠️ If you use this in a network service (API, SaaS, web app), you **must** open-source your entire application under AGPL-3.0
- 💼 Need to use this in proprietary software? Contact [your@email.com] for a commercial license

See the `LICENSE` file for full terms.

Read the license: https://www.gnu.org/licenses/agpl-3.0.en.html

**Why AGPL?** This ensures that if companies build services using this wallet, they contribute back to the community by open-sourcing their code. If you want to use this commercially without open-sourcing, please contact us for licensing options.

## Disclaimer

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

**You are solely responsible for:**
- Securing your private keys
- Any loss of funds
- Understanding the code before using it
- Complying with applicable laws and regulations

## Links

- [X402 Protocol](https://x402.org)
- [X402 Documentation](https://x402.gitbook.io/x402)
- [EIP-3009 Specification](https://eips.ethereum.org/EIPS/eip-3009)
- [Recaipe API](https://app.recaipe.com) - Example x402-protected service
- [AGPL-3.0 License](https://www.gnu.org/licenses/agpl-3.0.en.html)

## Support

- GitHub Issues: https://github.com/0xkoda/x402-wallet/issues

---

**Built for the x402 ecosystem** - Making APIs payable with crypto, one request at a time.