import { Routes, Route } from "react-router-dom";
import { Layout } from "./Layout";
import { Wall } from "./Wall";
import { Play } from "./Play";
import { Permalink } from "./Permalink";
import { ProfilePage } from "./ProfilePage";
import { Tos } from "./Tos";
import { Privacy } from "./Privacy";
import { Docs } from "./Docs";
import { Studio } from "../studio/Studio";
import { AdminPage } from "./AdminPage";
import { Browse } from "./Browse";
import { StudioRawOutput } from "../studio/output/StudioRawOutput";

export function AppRoutes() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<Wall />} />
        <Route path="/t/:id" element={<Permalink />} />
        <Route path="/u/:handle" element={<ProfilePage />} />
        <Route path="/tos" element={<Tos />} />
        <Route path="/privacy" element={<Privacy />} />
        <Route path="/docs" element={<Docs />} />
        <Route path="/admin" element={<AdminPage />} />
        <Route path="/browse" element={<Browse />} />
      </Route>
      <Route path="/t/:id/play" element={<Play />} />
      <Route path="/t/:id/raw" element={<Permalink raw />} />
      <Route path="/studio/raw" element={<StudioRawOutput />} />
      {/* Studio owns the full viewport — outside the nav shell so its layout is unchanged. */}
      <Route path="/studio" element={<Studio />} />
    </Routes>
  );
}
