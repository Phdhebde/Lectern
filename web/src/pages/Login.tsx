import { useEffect, useRef, useState, type FormEvent } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { api } from "../lib/api";
import { t } from "../lib/i18n";
import { useSession } from "../lib/session";
import { ErrorBox } from "../components/ui";

export function Login() {
  const { instance, me } = useSession();
  const [params] = useSearchParams();
  const returnTo = params.get("return_to") ?? "/";
  const [email, setEmail] = useState("");
  const [name, setName] = useState("");
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const navigate = useNavigate();

  useEffect(() => {
    if (me) navigate(returnTo, { replace: true });
  }, [me, navigate, returnTo]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.post("/api/auth/email/request", { email, display_name: name, return_to: returnTo });
      setSent(true);
    } catch (err) {
      setError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="card narrow">
      <h1>{t("auth.title", { instance: instance.name })}</h1>
      {params.get("error") && <div className="alert alert-danger">{t("errors.oidc")}</div>}
      {instance.auth.oidc && (
        <>
          <a className="button button-block" href={`${instance.auth.oidc.url}?return_to=${encodeURIComponent(returnTo)}`}>
            {t("auth.oidc", { label: instance.auth.oidc.label })}
          </a>
          {instance.auth.email && <p className="divider">{t("auth.or")}</p>}
        </>
      )}
      {instance.auth.email &&
        (sent ? (
          <div className="alert alert-success" role="status">
            {t("auth.link_sent", { email })}
          </div>
        ) : (
          <form onSubmit={submit} className="form">
            <label>
              {t("auth.email")}
              <input type="email" required autoComplete="email" value={email} onChange={(e) => setEmail(e.target.value)} />
            </label>
            <label>
              {t("auth.name")} <span className="muted">({t("auth.name_hint")})</span>
              <input type="text" autoComplete="name" value={name} onChange={(e) => setName(e.target.value)} maxLength={120} />
            </label>
            <ErrorBox error={error} />
            <button className="button button-block" disabled={busy}>
              {t("auth.send_link")}
            </button>
          </form>
        ))}
      <p className="muted small">{t("auth.privacy_notice")}</p>
    </div>
  );
}

/** Target of the e-mailed link. The token sits in the URL fragment and is POSTed once. */
export function EmailLogin() {
  const { refresh } = useSession();
  const navigate = useNavigate();
  const [error, setError] = useState<unknown>(null);
  const done = useRef(false);

  useEffect(() => {
    if (done.current) return;
    done.current = true;
    const token = new URLSearchParams(location.hash.slice(1)).get("token");
    history.replaceState(null, "", location.pathname);
    if (!token) {
      setError(new Error("missing token"));
      return;
    }
    api
      .post<{ return_to: string }>("/api/auth/email/verify", { token })
      .then(async (r) => {
        await refresh();
        navigate(r.return_to || "/", { replace: true });
      })
      .catch(setError);
  }, [navigate, refresh]);

  return <div className="card narrow">{error ? <ErrorBox error={error} /> : <p>{t("auth.signing_in")}</p>}</div>;
}
