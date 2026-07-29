import { useState, type FormEvent } from "react";

import { ApiError, api } from "../api/client";
import { formatCents } from "../lib/format";
import type {
  AssistantAnswer,
  HistoryTurn,
  PendingPurchase,
  Product,
} from "../api/types";
import { ProductCard } from "./ProductCard";

const EXAMPLES = [
  "find me the stools under 100",
  "I need a cheap chair, ideally something in white",
  "buy me the cheapest stool you can find",
];

interface Props {
  /** item_id of the product currently being bought, if any. */
  buyingId: string | null;
  /** Null while the balance is still loading. */
  balanceCents: number | null;
  onBuy: (product: Product) => void;
  /** Called after a confirmed order, so the header balance stays in step. */
  onBalanceChanged: (remainingCents: number) => void;
}

function askErrorMessage(error: unknown): string {
  if (!(error instanceof ApiError)) {
    return "Something went wrong. Please try again.";
  }
  switch (error.code) {
    case "upstream_error":
      // Covers assistant timeouts, rate limits, and a missing API key — the
      // backend's message is already specific, so pass it through.
      return error.message;
    case "network":
      return "Could not reach the server. Check that the API is running.";
    default:
      return error.message;
  }
}

/**
 * Plain-English request box.
 *
 * The shop's API can only filter by exact category, so anything qualitative —
 * "cheap", a colour, "for a small flat" — is judged by the assistant over the
 * rows it fetches. Its picks come back as full product records and render as
 * the same cards used by the catalogue grid.
 */
export function AssistantBox({
  buyingId,
  balanceCents,
  onBuy,
  onBalanceChanged,
}: Props) {
  const [message, setMessage] = useState("");
  const [answer, setAnswer] = useState<AssistantAnswer | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  /** The proposal on screen, echoed back so "yes" confirms *this* purchase. */
  const [pending, setPending] = useState<PendingPurchase | null>(null);
  /** Replayed to the backend so follow-ups like "the third one" resolve. */
  const [history, setHistory] = useState<HistoryTurn[]>([]);
  /** What the user has said, for the on-screen conversation. */
  const [saidSoFar, setSaidSoFar] = useState<string[]>([]);

  const submit = async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed || asking) return;

    setAsking(true);
    setError(null);
    setAnswer(null);
    setSaidSoFar((prior) => [...prior, trimmed]);
    try {
      const result = await api.ask(trimmed, pending, history);
      setAnswer(result);
      // Carry a new proposal forward; clear it once settled either way.
      setPending(result.pending_purchase);
      setHistory((prior) => [
        ...prior,
        { role: "user", text: trimmed },
        { role: "assistant", text: result.transcript },
      ]);
      if (result.order_placed) {
        onBalanceChanged(result.order_placed.remaining_balance_cents);
      }
      setMessage("");
    } catch (err) {
      setError(askErrorMessage(err));
      // Drop the turn we optimistically added; it never reached the assistant.
      setSaidSoFar((prior) => prior.slice(0, -1));
    } finally {
      setAsking(false);
    }
  };

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    void submit(message);
  };

  /** Starts a fresh conversation — the assistant forgets what came before. */
  const clear = () => {
    setAnswer(null);
    setMessage("");
    setPending(null);
    setError(null);
    setHistory([]);
    setSaidSoFar([]);
  };

  // Everything the user said before the turn on screen.
  const earlier = saidSoFar.slice(0, -1);

  return (
    <section className="assistant">
      <form onSubmit={handleSubmit}>
        <label className="assistant-label" htmlFor="assistant-input">
          Ask for what you need
        </label>
        <div className="assistant-row">
          <input
            id="assistant-input"
            type="text"
            placeholder={
              pending
                ? "Type Yes to confirm, or No to cancel"
                : "e.g. find me the stools under 100"
            }
            value={message}
            disabled={asking}
            onChange={(e) => setMessage(e.target.value)}
          />
          <button
            type="submit"
            className="primary"
            disabled={asking || !message.trim()}
          >
            {asking ? "Working…" : pending ? "Reply" : "Ask"}
          </button>
        </div>

        {!answer && !asking && (
          <div className="assistant-examples">
            {EXAMPLES.map((example) => (
              <button
                key={example}
                type="button"
                className="chip"
                onClick={() => {
                  setMessage(example);
                  void submit(example);
                }}
              >
                {example}
              </button>
            ))}
          </div>
        )}
      </form>

      {/* Earlier turns, so it's visible that the assistant still has context. */}
      {earlier.length > 0 && (
        <ul className="assistant-earlier">
          {earlier.map((said, index) => (
            <li key={`${index}-${said}`} className="muted">
              You asked: “{said}”
            </li>
          ))}
        </ul>
      )}

      {asking && (
        <p className="muted">
          {pending ? "Placing the order…" : "Searching the catalogue…"}
        </p>
      )}

      {error && <p className="error">{error}</p>}

      {answer && (
        <div className="assistant-answer">
          {answer.steps.length > 0 && (
            <ul className="assistant-steps">
              {answer.steps.map((step, index) => (
                <li
                  key={`${step.tool}-${index}`}
                  className={step.is_error ? "error" : "muted"}
                >
                  {step.summary}
                </li>
              ))}
            </ul>
          )}

          {answer.summary && (
            <p className="assistant-reply">{answer.summary}</p>
          )}

          {pending && (
            <div className="confirm-panel">
              <p className="confirm-title">
                About to buy — nothing has been charged yet
              </p>
              <dl className="confirm-figures">
                <div>
                  <dt>Item</dt>
                  <dd>
                    {pending.quantity} × {pending.product_name}
                    <span className="muted"> ({pending.item_id})</span>
                  </dd>
                </div>
                <div>
                  <dt>Each</dt>
                  <dd>{formatCents(pending.price_cents)}</dd>
                </div>
                <div>
                  <dt>Total</dt>
                  <dd>
                    <strong>{formatCents(pending.total_cents)}</strong>
                  </dd>
                </div>
                <div>
                  <dt>Balance after</dt>
                  <dd className={pending.affordable ? undefined : "error"}>
                    {formatCents(pending.balance_after_cents)}
                  </dd>
                </div>
              </dl>

              {pending.affordable ? (
                <p className="confirm-prompt">
                  Type <strong>Yes</strong> to place this order, or{" "}
                  <strong>No</strong> to cancel.
                </p>
              ) : (
                <p className="error">
                  Your balance of {formatCents(pending.balance_cents)} does not
                  cover this, so it can't be ordered.
                </p>
              )}
            </div>
          )}

          {answer.order_placed && (
            <p className="notice">
              Ordered {answer.order_placed.quantity} ×{" "}
              {answer.order_placed.product_name} for{" "}
              {formatCents(answer.order_placed.total_cents)}. Balance now{" "}
              {formatCents(answer.order_placed.remaining_balance_cents)}.
            </p>
          )}

          {answer.recommendations.length > 0 ? (
            <div className="grid assistant-grid">
              {answer.recommendations.map((rec) => (
                <div key={rec.item_id} className="assistant-pick">
                  <ProductCard
                    product={rec}
                    buying={buyingId === rec.item_id}
                    anyBuying={buyingId !== null}
                    affordable={
                      balanceCents === null || rec.price_cents <= balanceCents
                    }
                    onBuy={onBuy}
                  />
                  <p className="assistant-reason muted">{rec.reason}</p>
                </div>
              ))}
            </div>
          ) : (
            <p className="muted">No matching products.</p>
          )}

          <button type="button" className="link-button" onClick={clear}>
            Start over
          </button>
        </div>
      )}
    </section>
  );
}
