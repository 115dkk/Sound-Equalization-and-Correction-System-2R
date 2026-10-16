import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Gallery } from "./Gallery";
import { mountTour } from "./tour/TourPage";
import "../styles/base.css";
import "./gallery.css";

// gallery.html?tour=… is the screen tour (the whole App in one window, as
// the app draws it); anything else is the specimen gallery, whose page
// rules (gallery.css) apply only under html[data-gallery-page].
const root = document.getElementById("root");
const params = new URLSearchParams(window.location.search);
if (root && params.has("tour")) {
  mountTour(root, params);
} else if (root) {
  document.documentElement.dataset.galleryPage = "";
  createRoot(root).render(
    <StrictMode>
      <Gallery />
    </StrictMode>,
  );
}
