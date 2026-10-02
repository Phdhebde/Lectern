import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router";
import "./styles.css";
import { Layout, RequireLogin } from "./components/Layout";
import { SessionProvider } from "./lib/session";
import { Catalog } from "./pages/Catalog";
import { Login, EmailLogin } from "./pages/Login";
import { TrackPage } from "./pages/Track";
import { ModulePage } from "./pages/Module";
import { ScenarioPage } from "./pages/Scenario";
import { ExamPage } from "./pages/Exam";
import { CertificationsPage } from "./pages/Certifications";
import { ProfilePage } from "./pages/Profile";
import { OrganizationPage } from "./pages/Organization";
import { PartnersPage, PartnerDetail } from "./pages/Partners";
import { ReviewsPage, ReviewDetail } from "./pages/Reviews";
import { AdminPage } from "./pages/admin/Admin";
import { TrackEditor } from "./pages/admin/TrackEditor";
import { NotFound } from "./pages/NotFound";

const auth = (el: React.ReactNode) => <RequireLogin>{el}</RequireLogin>;

const router = createBrowserRouter([
  {
    element: <Layout />,
    children: [
      { path: "/", element: <Catalog /> },
      { path: "/login", element: <Login /> },
      { path: "/login/email", element: <EmailLogin /> },
      { path: "/tracks/:slug", element: <TrackPage /> },
      { path: "/tracks/:slug/modules/:module", element: auth(<ModulePage />) },
      { path: "/tracks/:slug/scenarios/:scenario", element: auth(<ScenarioPage />) },
      { path: "/exam/:id", element: auth(<ExamPage />) },
      { path: "/certifications", element: auth(<CertificationsPage />) },
      { path: "/profile", element: auth(<ProfilePage />) },
      { path: "/organization", element: auth(<OrganizationPage />) },
      { path: "/partners", element: auth(<PartnersPage />) },
      { path: "/partners/:id", element: auth(<PartnerDetail />) },
      { path: "/reviews", element: auth(<ReviewsPage />) },
      { path: "/reviews/:id", element: auth(<ReviewDetail />) },
      { path: "/admin", element: auth(<AdminPage />) },
      { path: "/admin/tracks/:slug", element: auth(<TrackEditor />) },
      { path: "*", element: <NotFound /> },
    ],
  },
]);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <SessionProvider fallback={<div className="boot" />}>
      <RouterProvider router={router} />
    </SessionProvider>
  </StrictMode>,
);
