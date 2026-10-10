# Stock analyser: product brief

2026-10-10

A workspace for people who follow a handful of stocks. An AI agent reads the watchlist every day, marks the chart with where it would buy and sell, explains the news, and answers questions about what it sees. It trades on paper, so every call it makes is scored against the market over time. The user acts in their own broker app.

## The problem

Someone who follows twenty stocks has the opposite of an information problem. Prices, news, ratios and opinions are everywhere and mostly free. What they do not have is time, and a way to tell which of today's twenty headlines changes anything.

The AI tools that answer this each fall short in the same way. Scoring services hand over a number with no reasoning behind it. Chat assistants give a fluent answer that vanishes when the window scrolls. Nothing keeps what it told you, so nothing can be checked later. The user ends up trusting a tool they have no way to judge.

The brokers made this worse by shipping chat. A chat window is good at answering one question and bad at being a place you return to. There is no chart, no history, no second look at what the AI said three weeks ago.

## Who it is for

**The user:** someone who already invests their own money, follows roughly ten to thirty tickers, and checks on them a few times a week. They know what a moving average is. They want a second opinion that shows its work, and they want to judge that opinion over time before they trust it.

**Not for:** day traders, who need live data and sub-second tools this will never have. Not for complete beginners, who want to be told what to buy and are the people a product like this can hurt most. Not for anyone who wants their money managed, which is a licensed business.

In Germany the pool is large enough. Private households held [37.2 million securities depots](https://www.boersen-zeitung.de/banken-finanzen/broker-haengen-banken-bei-depots-ab) at the end of 2025, and online and neo brokers hold 16.2 million of those. The self-directed slice is the fast-growing one, up 15.2% in a year.

## What it is

One page per ticker, with four things on it:

- **A chart carrying the agent's opinion.** Support, resistance, entry zones and stops, drawn as lines and bands. Hover one and read why it is there. The agent returns these in a fixed schema, so it draws rather than describes.
- **A short read on the day,** three or four sentences on what is going on with this ticker.
- **News with a take.** Recent headlines, each with one line on whether it matters.
- **A chat box** that answers from the analysis already on the page, not from a fresh run.

Underneath, a paper portfolio. When the agent would buy, it places a paper order. Fills happen at the next open with costs subtracted, so the record cannot flatter itself. Over months the chart carries its own history: here is what it said in October, here is what the price did.

Two ways in. Connect your own Claude or ChatGPT through an MCP server, and your tokens pay for it. Or use the hosted agent, where the model is our choice and our cost.

## Why it can live next to the brokers

The brokers got to agents first. [Scalable Capital](https://www.trendingtopics.eu/scalable-capital-opens-its-brokerage-accounts-to-ai-agents-like-claude-grok-and-chatgpt/) opened its depots to ChatGPT, Claude and Grok in August 2026. [Robinhood](https://finance.yahoo.com/markets/stocks/article/robinhood-is-rolling-out-agentic-ai-trading-accounts-for-the-masses-230517174.html) went live with agentic accounts on 30 September 2026 and lets the agent trade without per-trade approval. Competing on "connect your agent" is lost before it starts.

Three things they are structurally unlikely to do:

- **Publish how wrong their AI was.** A bank cannot ship a page that scores its own assistant against the market. We can, because it is the whole point and we never touch anyone's money.
- **Work across brokers.** Each broker's agent sees its own accounts. Someone with a depot at two places has no single view, and no broker wants to build one.
- **Build a workspace.** Chat is the cheapest thing to add to an existing app, and it is where they will stop. A chart marked up by an agent, with history, is a product in its own right, not a feature.

The independent tools are not the obstacle either. Danelfin sells a score, Trade Ideas sells signals, Composer sells automation and is [US only](https://www.pricemyai.com/pricing/composer/). None of them sits between the chart and the user as an assistant that keeps its receipts.

## Money

**What the market pays.** The independent tools sit between 17 and 89 US dollars a month: [Danelfin from 17](https://www.pricemyai.com/pricing/danelfin/), [Composer 24](https://www.pricemyai.com/pricing/composer/), [Trade Ideas 89](https://www.pricemyai.com/compare/composer-vs-trade-ideas/), all billed annually, checked 10 June 2026. For an unknown tool the realistic ceiling is the lower end, so 20 to 30 euro a month.

**What it costs to serve one user.** Twenty tickers analysed once a day is twenty model calls with a fair amount of context. On a cheap model that is cents a day. The daily analysis is cached, so reading the page costs nothing and only runs cost money. Chat is the variable part and needs a cap.

**Three tiers that follow the cost, not the features:**

- **Bring your own agent — free.** The user connects their own Claude or ChatGPT through MCP. Their tokens, our storage. This tier also happens to be the one nobody else offers.
- **Hosted, cheap model — around 20 euro a month.** We run the daily analysis for them.
- **Hosted, better model — more.** Same product, stronger analysis and more chat.

**What it adds up to.** 1,000 paying users at 20 euro is 240,000 euro a year. That is a real small business and a long way off. 100 users is 24,000 a year, which is the first number worth aiming at, and it needs 100 people to pick an unknown tool over their broker's built-in one.

## What would prove it works

The central bet is that people want a second opinion that keeps its receipts. That is testable, cheaply, and before any marketing.

**The first test is on yourself.** Run it on your own watchlist for four weeks. If you stop opening the history page after week one, nobody else will open it either, and the product is a scoring tool with extra steps.

**Then, with ten invited users over a month:**

- Do they come back without being prompted? Weekly return is the only engagement number that matters here.
- Do they add tickers after the first session? A watchlist that never grows means they tried it once.
- Do they open an old analysis? This is the direct test of the whole idea.
- Do they ask follow-up questions in chat, or just read and leave?

**What would say it does not work:** people read the daily summary, never look at history, and would have been equally happy with a free score. If that is what happens, the honest move is to admit the record is not the draw and rebuild around whatever they did use.

Note that the agent beating the market is *not* a success criterion. It probably will not, most of the time. The product is the visible reasoning and the honest scoreboard, and those have value whether the calls are good or bad.

## Risks

- **The track record is unflattering.** The agent underperforms buy-and-hold, visibly, on our own page. Mitigation: say so up front and sell the reasoning, not the returns. A tool that is honest about being wrong is more useful than one that hides it. But this will cost sign-ups, and pretending otherwise would be dishonest planning.
- **Scope creep toward real trading.** It is the fun part and it is the licensed part. The warning sign is the first line of code that calls an order endpoint.
- **Someone trades real money on it and loses.** Unavoidable, partly. Disclaimers on every page, no personalised recommendations, no suitability questionnaire, and a product shaped so the user has to act themselves.
- **The free data breaks.** The Yahoo endpoint is unofficial and can change without notice. Mitigation: one trait in front of it and a paid API ready to swap in.
- **A broker ships the workspace.** Scalable or another one builds the marked-up chart with history. Then the differentiator is cross-broker and independence, both weaker. Nothing to do but be early and small.
- **It stays a side project.** The real risk, honestly. A product that needs daily data, uptime and support is not a thing to run in the gaps between an Ausbildung and job applications. Worth deciding now how much of the week it gets, and keeping the first milestone small enough to finish.
