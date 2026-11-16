import { useEffect, useRef, useState } from "react";
import { AppBar, useApp } from "@/app";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, type SearchResponse, type SearchScope, type SearchStatus } from "@/lib/api";
import { t } from "@/lib/i18n";

export function SearchView() {
  const { publications, push } = useApp();
  const [query, setQuery] = useState("");
  const [scope, setScope] = useState("all");
  const [offset, setOffset] = useState(0);
  const [response, setResponse] = useState<SearchResponse | null>(null);
  const [status, setStatus] = useState<SearchStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [retrying, setRetrying] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [indexVersion, setIndexVersion] = useState(0);
  const previousStatus = useRef<SearchStatus | null>(null);

  useEffect(() => {
    let current = true;
    const poll = () => api.searchStatus().then((value) => {
      if (!current) return;
      if (previousStatus.current?.state === "indexing" && value.state !== "indexing") setIndexVersion((v) => v + 1);
      previousStatus.current = value;
      setStatus(value);
    }).catch((e) => { if (current) setError(String(e)); });
    void poll();
    const interval = window.setInterval(poll, 2000);
    return () => { current = false; window.clearInterval(interval); };
  }, [refresh]);

  useEffect(() => {
    let current = true;
    setResponse(null);
    setError(null);
    if (!query.trim()) {
      setLoading(false);
      return () => { current = false; };
    }
    if ([...query].filter((character) => character === '"').length % 2 !== 0) {
      setLoading(false);
      setError(t("Unmatched quotation mark"));
      return () => { current = false; };
    }
    const selected: SearchScope = scope === "all" ? { kind: "all" } : scope === "bibles" ? { kind: "bibles" } : { kind: "publication", dirName: scope.slice("publication:".length) };
    setLoading(true);
    const timer = window.setTimeout(() => {
      api.searchPublications({ query, scope: selected, limit: 50, offset })
        .then((result) => { if (current) { setResponse(result); setStatus(result.status); } })
        .catch((e) => { if (current) setError(String(e)); })
        .finally(() => { if (current) setLoading(false); });
    }, 250);
    return () => { current = false; window.clearTimeout(timer); };
  }, [query, scope, offset, refresh, indexVersion]);

  const retry = async () => {
    setRetrying(true);
    try {
      await api.rebuildSearch();
      setRefresh((value) => value + 1);
    } catch (e) {
      setError(String(e));
    } finally {
      setRetrying(false);
    }
  };

  return (
    <>
      <AppBar title={t("Search library")} />
      <div className="page flex-1 overflow-y-auto">
        <div className="flex max-w-3xl flex-wrap gap-2">
          <Input type="search" aria-label={t("Search library")} value={query} onChange={(event) => { setOffset(0); setQuery(event.target.value); }} className="min-w-48 flex-1" />
          <select aria-label={t("Search scope")} value={scope} onChange={(event) => { setOffset(0); setScope(event.target.value); }} className="h-9 rounded-md border border-line bg-surface px-3 text-sm">
            <option value="all">{t("All downloaded content")}</option>
            <option value="bibles">{t("Downloaded Bibles")}</option>
            {publications.map((publication) => <option key={publication.dir} value={`publication:${publication.dir}`}>{publication.title}</option>)}
          </select>
        </div>
        {status && status.state !== "ready" && (
          <div role="status" className="mt-4 max-w-3xl text-sm text-muted">
            {status.state === "indexing" ? t("Indexing downloaded content…") : status.state === "unavailable" ? t("Search is unavailable on this device.") : t("Search index is incomplete; some content may be missing.")}
            {status.state === "incomplete" && <Button variant="outline" size="sm" className="ml-3" disabled={retrying} onClick={() => void retry()}>{t("Retry indexing")}</Button>}
          </div>
        )}
        <div className="mt-6 max-w-3xl" aria-live="polite">
          {loading && <p className="text-muted">{t("Searching…")}</p>}
          {error && <p role="alert">{error}</p>}
          {response && response.hits.length === 0 && <p className="text-muted">{t("Nothing found.")}</p>}
          {response?.hits.map((hit, index) => (
            <button key={index} className="block w-full border-b border-line py-3 text-left hover:bg-bar" onClick={() => push({ name: "reader", target: hit.target, fromSearch: true })}>
              <span className="block font-semibold">{hit.title}</span>
              <span className="block text-sm text-muted">{hit.publicationTitle}</span>
              <span className="block text-sm">{hit.snippet}</span>
            </button>
          ))}
          {response && (offset > 0 || response.hits.length === 50) && (
            <div className="mt-4 flex gap-2">
              <Button variant="outline" disabled={offset === 0} onClick={() => setOffset((value) => Math.max(0, value - 50))}>{t("Previous page")}</Button>
              <Button variant="outline" disabled={response.hits.length < 50} onClick={() => setOffset((value) => value + 50)}>{t("Next page")}</Button>
            </div>
          )}
        </div>
      </div>
    </>
  );
}
