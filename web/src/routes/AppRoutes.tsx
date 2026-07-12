import { Routes, Route } from "react-router-dom";
import { Layout } from "./Layout";
import { Wall } from "./Wall";
import { Permalink } from "./Permalink";
import { ProfilePage } from "./ProfilePage";
import { Tos } from "./Tos";
import { Privacy } from "./Privacy";
import { Studio } from "../studio/Studio";

export function AppRoutes() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<Wall />} />
        <Route path="/t/:id" element={<Permalink />} />
        <Route path="/u/:handle" element={<ProfilePage />} />
        <Route path="/tos" element={<Tos />} />
        <Route path="/privacy" element={<Privacy />} />
      </Route>
      {/* Studio owns the full viewport — outside the nav shell so its layout is unchanged. */}
      <Route path="/studio" element={<Studio />} />
    </Routes>
  );
}
