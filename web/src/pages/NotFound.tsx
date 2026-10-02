import { Link } from "react-router";
import { t } from "../lib/i18n";

export function NotFound() {
  return (
    <div className="card narrow">
      <h1>{t("errors.not_found")}</h1>
      <Link to="/">{t("nav.catalog")}</Link>
    </div>
  );
}
