import { Link, NavLink, Outlet } from "react-router";
import { t } from "../lib/i18n";
import { useSession } from "../lib/session";

export function Layout() {
  const { instance, me, hasRole } = useSession();
  const manager = me?.membership?.status === "approved" && me.membership.org_role === "training_manager";
  return (
    <>
      <a className="skip-link" href="#main">
        {t("nav.skip")}
      </a>
      <header className="site-header">
        <div className="container header-inner">
          <Link to="/" className="brand">
            {instance.logo && <img src={instance.logo} alt="" />}
            <span>{instance.name}</span>
          </Link>
          <nav aria-label={t("nav.main")}>
            <NavLink to="/" end>
              {t("nav.catalog")}
            </NavLink>
            {me && <NavLink to="/certifications">{t("nav.certifications")}</NavLink>}
            {manager && <NavLink to="/organization">{t("nav.organization")}</NavLink>}
            {(hasRole("channel_manager") || hasRole("admin")) && <NavLink to="/partners">{t("nav.partners")}</NavLink>}
            {hasRole("trainer") && (
              <NavLink to="/reviews">
                {t("nav.reviews")}
                {me && me.pending_reviews > 0 && <span className="count">{me.pending_reviews}</span>}
              </NavLink>
            )}
            {(hasRole("admin") || hasRole("trainer")) && <NavLink to="/admin">{t("nav.admin")}</NavLink>}
            {me ? (
              <NavLink to="/profile" className="nav-profile">
                {me.display_name}
              </NavLink>
            ) : (
              <NavLink to="/login" className="button button-small">
                {t("nav.login")}
              </NavLink>
            )}
          </nav>
        </div>
      </header>
      <main id="main" className="container">
        <Outlet />
      </main>
      <footer className="site-footer">
        <div className="container footer-inner">
          <span>{instance.name}</span>
          <a href={`mailto:${instance.contact_email}`}>{t("footer.contact")}</a>
          {instance.documentation_url && <a href={instance.documentation_url}>{t("footer.documentation")}</a>}
          {instance.legal_notice_url && <a href={instance.legal_notice_url}>{t("footer.legal")}</a>}
          {instance.privacy_policy_url && <a href={instance.privacy_policy_url}>{t("footer.privacy")}</a>}
        </div>
      </footer>
    </>
  );
}

/** Wraps pages that need a signed-in user. */
export function RequireLogin({ children }: { children: React.ReactNode }) {
  const { me } = useSession();
  if (!me) {
    return (
      <div className="card narrow">
        <p>{t("auth.required")}</p>
        <Link className="button" to={`/login?return_to=${encodeURIComponent(location.pathname)}`}>
          {t("nav.login")}
        </Link>
      </div>
    );
  }
  return <>{children}</>;
}
