# X402 Protected Resources

A curated list of x402-protected APIs and services you have access to. Use these with the x402-wallet CLI following the workflow in `wallet.md`.

---

## Available Resources

### Recaipe API

**Endpoint:** `https://app.recaipe.com/api/recipes`

**Method:** `POST`

**What it does:** AI-powered recipe generation. Submit a query or chat history and receive structured recipes with ingredients, instructions, and tags.

**Request format:**

    {
      "history": [
        {"role": "user", "content": "your recipe query here"}
      ]
    }

**Example:**

    curl -X POST \
      -H "Content-Type: application/json" \
      -d '{"history":[{"role":"user","content":"quick vegan dinner"}]}' \
      https://app.recaipe.com/api/recipes

**Note:** You can use multi-turn conversations by adding multiple messages to the history array with alternating user/assistant roles.

---

## Adding Resources

To add a new x402-protected resource to this list:

1. **Endpoint** - Full URL
2. **Method** - HTTP method (GET, POST, etc.)
3. **What it does** - Brief description
4. **Request format** - Expected payload structure with example

Submit a pull request with your addition.

---

## Usage

When you want to use any of these resources:

1. Make an initial request (you'll get a 402 response with payment details)
2. Follow the payment workflow in `wallet.md`
3. Retry your request with the payment header

Payment details, costs, and network requirements are provided in the 402 response from each service.