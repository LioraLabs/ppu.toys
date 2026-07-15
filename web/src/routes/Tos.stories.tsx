import type { Story, StoryDefault } from "@ladle/react";
import { Tos } from "./Tos";
import "./layout.css"; // .doc-page styling lives here

// Static content page — pure markup, no data. Import the layout CSS so the
// .doc-page wrapper is styled in the catalog.
export default {
  title: "Pages/Tos",
} satisfies StoryDefault;

export const Default: Story = () => <Tos />;
