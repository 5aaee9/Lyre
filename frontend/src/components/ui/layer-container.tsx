"use client";

import { createContext, useContext } from "react";

export const LayerContainerContext = createContext<HTMLElement | null>(null);

export function useLayerContainer(): HTMLElement | null {
  return useContext(LayerContainerContext);
}
