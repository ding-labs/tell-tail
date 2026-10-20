import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

// An explicit module directory permits using a preinstalled development runtime.
const specifier = process.env.PLAYWRIGHT_MODULE_DIR
  ? pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_DIR, 'index.mjs')).href : 'playwright';
export const { chromium } = await import(specifier);
export const launchOptions = {
  headless: true,
  args: ['--disable-background-timer-throttling', '--disable-backgrounding-occluded-windows', '--disable-renderer-backgrounding'],
};
