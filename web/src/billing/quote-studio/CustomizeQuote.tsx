import { useRef } from "react";
import type React from "react";
import { Building2, Check, ContactRound, FileText, Globe2, Link, Mail, Palette, Phone, QrCode, RotateCcw, Table2, Upload, X } from "lucide-react";
import { Button, Modal, cx } from "../../ds";
import { strings } from "../../i18n";
import { readBrandKit } from "../../branding/repository";
import { importBrandQuoteColors } from "./brandQuoteColors";
import { importBrandQuoteTypography, themeQuoteTypography } from "./quoteTypography";
import { ColorField } from "./ColorField";
import { HeaderField } from "./HeaderField";
import { HeaderStylePreview, type HeaderStyle } from "./HeaderStylePreview";
import {
  DEFAULT_QUOTE_COLORS,
  type QuoteCustomerHeaderDetails as CustomerHeaderDetails,
  type QuoteHeaderDetails as HeaderDetails,
  type QuoteStudioColors as Colors,
  type QuoteStudioDesign as Design,
  type QuoteStudioTheme as Theme,
} from "./QuoteStudioDesign";
import { HEADER_RATIO_CHOICES as headerRatioChoices } from "./headerRatioChoices";
import { readQuoteImage as imageData } from "./quoteImageData";
export function CustomizeQuote({
  mode,
  design,
  issuerDetails,
  customerDetails: sourceCustomerDetails,
  saveError,
  onChange,
  onClose,
}: {
  mode: "header" | "document";
  design: Design;
  issuerDetails: HeaderDetails;
  customerDetails: CustomerHeaderDetails;
  saveError: string;
  onChange: React.Dispatch<React.SetStateAction<Design>>;
  onClose: () => void;
}) {
  const logoInput = useRef<HTMLInputElement>(null);
  const themeChoices: Array<{ id: Theme; name: string; help: string }> = [
    {
      id: "modern",
      name: strings.quoteStudioModern,
      help: strings.quoteStudioModernHelp,
    },
    {
      id: "editorial",
      name: strings.quoteStudioEditorial,
      help: strings.quoteStudioEditorialHelp,
    },
    {
      id: "minimal",
      name: strings.quoteStudioMinimal,
      help: strings.quoteStudioMinimalHelp,
    },
  ];
  const headerStyleChoices: Array<{
    id: HeaderStyle;
    name: string;
    help: string;
  }> = [
    {
      id: "signature",
      name: strings.quoteStudioSignature,
      help: strings.quoteStudioSignatureHelp,
    },
    {
      id: "editorial",
      name: strings.quoteStudioEditorial,
      help: strings.quoteStudioHeaderEditorialHelp,
    },
    {
      id: "band",
      name: strings.quoteStudioBrandBand,
      help: strings.quoteStudioBrandBandHelp,
    },
    {
      id: "minimal",
      name: strings.quoteStudioMinimal,
      help: strings.quoteStudioHeaderMinimalHelp,
    },
    {
      id: "stacked",
      name: strings.quoteStudioLogoStack,
      help: strings.quoteStudioLogoStackHelp,
    },
  ];
  const setColor = (name: keyof Colors, value: string) =>
    onChange((current) => ({
      ...current,
      colors: { ...current.colors, [name]: value },
    }));
  const displayedHeaderDetails = design.headerDetailsCustomized
    ? design.headerDetails
    : issuerDetails;
  const setHeaderDetail = (name: keyof HeaderDetails, value: string) =>
    onChange((current) => ({
      ...current,
      headerDetails: { ...displayedHeaderDetails, [name]: value },
      headerDetailsCustomized: true,
    }));
  const displayedCustomerDetails = design.customerDetailsCustomized
    ? design.customerDetails
    : sourceCustomerDetails;
  const setCustomerDetail = (
    name: keyof CustomerHeaderDetails,
    value: string,
  ) =>
    onChange((current) => ({
      ...current,
      customerDetails: { ...displayedCustomerDetails, [name]: value },
      customerDetailsCustomized: true,
    }));
  return (
    <Modal
      title={
        mode === "header"
          ? strings.quoteStudioEditQuotationHeader
          : strings.quoteStudioCustomizeQuotation
      }
      icon={
        mode === "header" ? (
          <Building2 className="size-5" />
        ) : (
          <Palette className="size-5" />
        )
      }
      onClose={onClose}
      wide="extra"
      actions={
        <button
          type="button"
          className="flex size-9 items-center justify-center rounded-lg text-tertiary hover:bg-raised hover:text-primary"
          aria-label={strings.quoteStudioClose}
          onClick={onClose}
        >
          <X className="size-4" />
        </button>
      }
      footer={
        <div className="flex w-full items-center gap-3 px-1">
          <p
            className={cx(
              "mr-auto text-xs",
              saveError ? "text-danger" : "text-secondary",
            )}
          >
            {saveError || strings.quoteStudioChangesSavedAutomatically}
          </p>
          <Button onClick={onClose}>{strings.quoteStudioDone}</Button>
        </div>
      }
    >
      <div className="space-y-7 p-2">
        {mode === "header" && (
          <>
            <section className="flex flex-wrap items-center gap-5 rounded-2xl border border-default bg-raised/35 p-5">
              <div className="min-w-52 flex-1">
                <h3 className="text-base font-semibold text-primary">
                  {strings.quoteStudioBrandMark}
                </h3>
                <p className="mt-1 text-sm leading-relaxed text-secondary">
                  {strings.quoteStudioBrandMarkHelp}
                </p>
              </div>
              <button
                type="button"
                className="flex size-24 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-default bg-surface p-3 text-sm font-semibold text-secondary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25"
                onClick={() => logoInput.current?.click()}
              >
                {design.logo ? (
                  <img
                    src={design.logo}
                    alt={strings.quoteStudioQuoteLogo}
                    className="max-h-20 max-w-full object-contain"
                  />
                ) : (
                  <span className="flex flex-col items-center gap-3 text-center">
                    <span className="grid size-10 place-items-center rounded-xl bg-accent-soft text-accent">
                      <Upload className="size-5" />
                    </span>
                    <span>
                      <strong className="sr-only">
                        {strings.quoteStudioUploadLogo}
                      </strong>
                    </span>
                  </span>
                )}
              </button>
              <div className="flex shrink-0 items-center gap-2">
                <button
                  type="button"
                  className="inline-flex min-h-9 items-center gap-2 rounded-lg px-3 text-sm font-semibold text-accent transition-colors hover:bg-accent-soft hover:text-accent-hover disabled:cursor-not-allowed disabled:text-tertiary disabled:opacity-50"
                  onClick={() => logoInput.current?.click()}
                >
                  <Upload className="size-4" />
                  {design.logo
                    ? strings.quoteStudioReplace
                    : strings.quoteStudioChooseFile}
                </button>
                <button
                  type="button"
                  disabled={!design.logo}
                  className="min-h-9 rounded-lg px-3 text-sm font-semibold text-secondary transition-colors hover:bg-danger-tint hover:text-danger disabled:cursor-not-allowed disabled:text-tertiary disabled:opacity-40"
                  onClick={() =>
                    onChange((current) => ({ ...current, logo: "" }))
                  }
                >
                  {strings.quoteStudioRemove}
                </button>
              </div>
              <input
                ref={logoInput}
                type="file"
                accept="image/png,image/jpeg,image/webp,image/svg+xml"
                className="sr-only"
                onChange={(event) => {
                  const file = event.target.files?.[0];
                  if (file)
                    imageData(file, (logo) =>
                      onChange((current) => ({ ...current, logo })),
                    );
                }}
              />
            </section>
            <section className="rounded-2xl border border-subtle bg-surface p-6 shadow-sm sm:p-8">
              <div className="flex flex-wrap items-center justify-between gap-5">
                <div className="flex items-start gap-4">
                  <span className="grid size-12 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
                    <QrCode className="size-5" aria-hidden="true" />
                  </span>
                  <div>
                    <h3 className="text-xl font-semibold tracking-tight text-primary">
                      {strings.quoteStudioQrTitle}
                    </h3>
                    <p className="mt-2 text-sm leading-relaxed text-secondary">
                      {strings.quoteStudioQrHelp}
                    </p>
                  </div>
                </div>
                <button
                  type="button"
                  role="switch"
                  aria-checked={design.showContactQr}
                  className={cx(
                    "relative h-7 w-12 shrink-0 rounded-full border transition-colors focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15",
                    design.showContactQr
                      ? "border-accent bg-accent"
                      : "border-default bg-raised",
                  )}
                  onClick={() =>
                    onChange((current) => ({
                      ...current,
                      showContactQr: !current.showContactQr,
                    }))
                  }
                >
                  <span
                    className={cx(
                      "absolute top-1 size-5 rounded-full bg-white shadow-sm transition-[left]",
                      design.showContactQr ? "left-6" : "left-1",
                    )}
                  />
                  <span className="sr-only">{strings.quoteStudioShowQr}</span>
                </button>
              </div>
              <div className="mt-7 grid gap-7 xl:grid-cols-2">
                <fieldset>
                  <legend className="text-sm font-semibold text-primary">
                    {strings.quoteStudioPlacement}
                  </legend>
                  <p className="mt-1 text-xs text-secondary">
                    {strings.quoteStudioPlacementHelp}
                  </p>
                  <div className="mt-4 grid grid-cols-2 gap-3">
                    {(["left", "right"] as const).map((alignment) => (
                      <button
                        key={alignment}
                        type="button"
                        aria-pressed={design.contactQrAlignment === alignment}
                        aria-label={strings.quoteStudioQrPlacementA11y(
                          alignment === "left"
                            ? strings.quoteStudioLeft
                            : strings.quoteStudioRight,
                        )}
                        className={cx(
                          "group relative min-h-24 cursor-pointer rounded-xl border p-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15",
                          design.contactQrAlignment === alignment
                            ? "border-accent bg-accent-soft/40"
                            : "border-default bg-surface hover:border-accent hover:bg-accent-soft/20",
                        )}
                        onClick={() =>
                          onChange((current) => ({
                            ...current,
                            showContactQr: true,
                            contactQrAlignment: alignment,
                          }))
                        }
                      >
                        <span
                          className={cx(
                            "flex h-16 items-end gap-3 rounded-lg bg-raised p-3",
                            alignment === "right" && "flex-row-reverse",
                          )}
                          aria-hidden="true"
                        >
                          <span className="grid size-9 shrink-0 place-items-center rounded-md bg-surface text-accent ring-1 ring-default">
                            <QrCode className="size-6" />
                          </span>
                          <span className="mb-1 flex-1 space-y-2">
                            <span className="block h-1.5 w-full rounded-full bg-primary/15" />
                            <span className="block h-1.5 w-2/3 rounded-full bg-primary/10" />
                          </span>
                        </span>
                        <span className="mt-2 block text-center text-xs font-semibold text-primary">
                          {alignment === "left"
                            ? strings.quoteStudioLeft
                            : strings.quoteStudioRight}
                        </span>
                        <span
                          className={cx(
                            "absolute right-2 top-2 grid size-5 place-items-center rounded-full border transition-colors",
                            design.contactQrAlignment === alignment
                              ? "border-accent bg-accent text-white"
                              : "border-default bg-surface group-hover:border-accent",
                          )}
                          aria-hidden="true"
                        >
                          {design.contactQrAlignment === alignment && (
                            <Check className="size-3" strokeWidth={3} />
                          )}
                        </span>
                      </button>
                    ))}
                  </div>
                </fieldset>
                <fieldset>
                  <legend className="text-sm font-semibold text-primary">
                    {strings.quoteStudioSize}
                  </legend>
                  <p className="mt-1 text-xs text-secondary">
                    {strings.quoteStudioSizeHelp}
                  </p>
                  <div className="mt-4 grid grid-cols-3 gap-3">
                    {(["small", "medium", "large"] as const).map((size) => (
                      <button
                        key={size}
                        type="button"
                        aria-pressed={design.contactQrSize === size}
                        className={cx(
                          "group relative min-h-24 cursor-pointer rounded-xl border p-3 transition-colors focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15",
                          design.contactQrSize === size
                            ? "border-accent bg-accent-soft/40"
                            : "border-default bg-surface hover:border-accent hover:bg-accent-soft/20",
                        )}
                        onClick={() =>
                          onChange((current) => ({
                            ...current,
                            showContactQr: true,
                            contactQrSize: size,
                          }))
                        }
                      >
                        <span
                          className="flex h-12 items-center justify-center"
                          aria-hidden="true"
                        >
                          <QrCode
                            className={cx(
                              "text-accent",
                              size === "small"
                                ? "size-6"
                                : size === "large"
                                  ? "size-11"
                                  : "size-8",
                            )}
                          />
                        </span>
                        <span className="mt-2 block text-center text-xs font-semibold text-primary">
                          {size === "small"
                            ? strings.quoteStudioSmall
                            : size === "medium"
                              ? strings.quoteStudioMedium
                              : strings.quoteStudioLarge}
                        </span>
                        <span
                          className={cx(
                            "absolute right-2 top-2 grid size-5 place-items-center rounded-full border transition-colors",
                            design.contactQrSize === size
                              ? "border-accent bg-accent text-white"
                              : "border-default bg-surface group-hover:border-accent",
                          )}
                          aria-hidden="true"
                        >
                          {design.contactQrSize === size && (
                            <Check className="size-3" strokeWidth={3} />
                          )}
                        </span>
                      </button>
                    ))}
                  </div>
                </fieldset>
                <div className="xl:col-span-2">
                  <ColorField
                    label={strings.quoteStudioQrColour}
                    help={strings.quoteStudioQrColourHelp}
                    value={design.contactQrColor}
                    onChange={(contactQrColor) =>
                      onChange((current) => ({
                        ...current,
                        showContactQr: true,
                        contactQrColor,
                      }))
                    }
                  />
                </div>
              </div>
            </section>
            <section className="rounded-2xl border border-subtle bg-surface p-6 shadow-sm sm:p-8">
              <div>
                <div className="flex flex-wrap items-center justify-between gap-5">
                  <div className="flex items-start gap-4">
                    <span className="grid size-12 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
                      <Building2 className="size-5" aria-hidden="true" />
                    </span>
                    <div>
                      <h3 className="text-xl font-semibold tracking-tight text-primary">
                        {strings.quoteStudioCompanyInformation}
                      </h3>
                      <p className="mt-2 text-sm leading-relaxed text-secondary">
                        {strings.quoteStudioCompanyLinkedHelp}
                        <span className="block">
                          {strings.quoteStudioOverrideHelp}
                        </span>
                      </p>
                    </div>
                  </div>
                  {design.headerDetailsCustomized ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      icon={<RotateCcw aria-hidden="true" />}
                      onClick={() =>
                        onChange((current) => ({
                          ...current,
                          headerDetailsCustomized: false,
                        }))
                      }
                    >
                      {strings.quoteStudioUseYourDetails}
                    </Button>
                  ) : (
                    <span className="inline-flex min-h-10 items-center gap-2 rounded-full bg-accent-soft px-4 text-sm font-semibold text-accent">
                      <Link className="size-4" aria-hidden="true" />
                      {strings.quoteStudioLinkedYourDetails}
                    </span>
                  )}
                </div>
              </div>
              <div className="mt-8 grid gap-x-8 gap-y-5 sm:grid-cols-2">
                <HeaderField
                  label={strings.quoteStudioCompanyName}
                  icon={<Building2 />}
                  value={displayedHeaderDetails.companyName}
                  placeholder={strings.quoteStudioCompanyNamePlaceholder}
                  onChange={(value) => setHeaderDetail("companyName", value)}
                />
                <HeaderField
                  label={strings.quoteStudioWebsite}
                  icon={<Globe2 />}
                  value={displayedHeaderDetails.website}
                  placeholder={strings.quoteStudioWebsitePlaceholder}
                  onChange={(value) => setHeaderDetail("website", value)}
                />
                <HeaderField
                  label={strings.quoteStudioEmail}
                  icon={<Mail />}
                  value={displayedHeaderDetails.email}
                  placeholder={strings.quoteStudioEmailPlaceholder}
                  onChange={(value) => setHeaderDetail("email", value)}
                />
                <HeaderField
                  label={strings.quoteStudioPhone}
                  icon={<Phone />}
                  value={displayedHeaderDetails.phone}
                  placeholder={strings.quoteStudioPhonePlaceholder}
                  onChange={(value) => setHeaderDetail("phone", value)}
                />
                <label className="grid gap-2 sm:col-span-2">
                  <span className="text-sm font-semibold text-primary">
                    {strings.quoteStudioAddress}
                  </span>
                  <textarea
                    className="min-h-32 resize-y rounded-xl border border-default bg-surface px-4 py-4 text-base leading-relaxed text-primary placeholder:text-tertiary hover:border-accent focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/10"
                    value={displayedHeaderDetails.address}
                    placeholder={strings.quoteStudioAddressPlaceholder}
                    onChange={(event) =>
                      setHeaderDetail("address", event.target.value)
                    }
                  />
                </label>
                <HeaderField
                  label={strings.quoteStudioVatId}
                  value={displayedHeaderDetails.vatId}
                  placeholder={strings.quoteStudioVatPlaceholder}
                  onChange={(value) => setHeaderDetail("vatId", value)}
                />
                <HeaderField
                  label={strings.quoteStudioCompanyNumber}
                  value={displayedHeaderDetails.registrationNo}
                  placeholder={strings.quoteStudioCompanyNumberPlaceholder}
                  onChange={(value) => setHeaderDetail("registrationNo", value)}
                />
              </div>
            </section>
            <section className="rounded-2xl border border-subtle bg-surface p-6 shadow-sm sm:p-8">
              <div className="flex flex-wrap items-center justify-between gap-5">
                <div className="flex items-start gap-4">
                  <span className="grid size-12 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
                    <ContactRound className="size-5" aria-hidden="true" />
                  </span>
                  <div>
                    <h3 className="text-xl font-semibold tracking-tight text-primary">
                      {strings.quoteStudioCustomerInformation}
                    </h3>
                    <p className="mt-2 text-sm leading-relaxed text-secondary">
                      {strings.quoteStudioCustomerInformationHelp}
                      <span className="block">
                        {strings.quoteStudioCustomerOverrideHelp}
                      </span>
                    </p>
                  </div>
                </div>
                {design.customerDetailsCustomized ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<RotateCcw aria-hidden="true" />}
                    onClick={() =>
                      onChange((current) => ({
                        ...current,
                        customerDetailsCustomized: false,
                      }))
                    }
                  >
                    {strings.quoteStudioUseSelectedCustomer}
                  </Button>
                ) : (
                  <span className="inline-flex min-h-10 items-center gap-2 rounded-full bg-accent-soft px-4 text-sm font-semibold text-accent">
                    <Link className="size-4" aria-hidden="true" />
                    {strings.quoteStudioLinkedSelectedCustomer}
                  </span>
                )}
              </div>
              <div className="mt-8 grid gap-x-8 gap-y-5 sm:grid-cols-2">
                <HeaderField
                  label={strings.quoteStudioCompanyName}
                  icon={<Building2 />}
                  value={displayedCustomerDetails.companyName}
                  placeholder={strings.quoteStudioCustomerCompanyPlaceholder}
                  onChange={(value) => setCustomerDetail("companyName", value)}
                />
                <HeaderField
                  label={strings.quoteStudioContactPerson}
                  icon={<ContactRound />}
                  value={displayedCustomerDetails.contactName}
                  placeholder={strings.quoteStudioContactNamePlaceholder}
                  onChange={(value) => setCustomerDetail("contactName", value)}
                />
                <HeaderField
                  label={strings.quoteStudioEmail}
                  icon={<Mail />}
                  value={displayedCustomerDetails.email}
                  placeholder={strings.quoteStudioCustomerEmailPlaceholder}
                  onChange={(value) => setCustomerDetail("email", value)}
                />
                <HeaderField
                  label={strings.quoteStudioPhone}
                  icon={<Phone />}
                  value={displayedCustomerDetails.phone}
                  placeholder={strings.quoteStudioPhonePlaceholder}
                  onChange={(value) => setCustomerDetail("phone", value)}
                />
                <label className="grid gap-2 sm:col-span-2">
                  <span className="text-sm font-semibold text-primary">
                    {strings.quoteStudioAddress}
                  </span>
                  <textarea
                    className="min-h-32 resize-y rounded-xl border border-default bg-surface px-4 py-4 text-base leading-relaxed text-primary placeholder:text-tertiary hover:border-accent focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/10"
                    value={displayedCustomerDetails.address}
                    placeholder={strings.quoteStudioAddressPlaceholder}
                    onChange={(event) =>
                      setCustomerDetail("address", event.target.value)
                    }
                  />
                </label>
                <HeaderField
                  label={strings.quoteStudioVatId}
                  value={displayedCustomerDetails.vatId}
                  placeholder={strings.quoteStudioCustomerVatPlaceholder}
                  onChange={(value) => setCustomerDetail("vatId", value)}
                />
              </div>
            </section>
          </>
        )}
        <div className="min-w-0 space-y-7">
          {mode === "header" && (
            <>
              <section>
                <h3 className="text-base font-semibold text-primary">
                  {strings.quoteStudioHeaderStyle}
                </h3>
                <p className="mt-1 text-sm text-secondary">
                  {strings.quoteStudioHeaderStyleHelp}
                </p>
                <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-5">
                  {headerStyleChoices.map((choice) => (
                    <button
                      key={choice.id}
                      type="button"
                      aria-pressed={design.headerStyle === choice.id}
                      className={cx(
                        "relative min-h-40 rounded-2xl border p-4 text-left transition-colors hover:border-accent hover:bg-accent-soft/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
                        design.headerStyle === choice.id
                          ? "border-accent bg-accent-soft/25"
                          : "border-default bg-surface",
                      )}
                      onClick={() =>
                        onChange((current) => ({
                          ...current,
                          headerStyle: choice.id,
                        }))
                      }
                    >
                      <HeaderStylePreview style={choice.id} />
                      <span className="mt-4 flex items-start justify-between gap-3">
                        <span>
                          <strong className="block text-sm font-semibold text-primary">
                            {choice.name}
                          </strong>
                          <small className="mt-1 block text-xs leading-relaxed text-secondary">
                            {choice.help}
                          </small>
                        </span>
                        <span
                          className={cx(
                            "grid size-5 shrink-0 place-items-center rounded-full border",
                            design.headerStyle === choice.id
                              ? "border-accent bg-accent text-white"
                              : "border-default",
                          )}
                        >
                          {design.headerStyle === choice.id && (
                            <Check className="size-3" />
                          )}
                        </span>
                      </span>
                    </button>
                  ))}
                </div>
              </section>
              <section>
                <div>
                  <h3 className="text-base font-semibold text-primary">
                    {strings.quoteStudioHeaderArrangement}
                  </h3>
                  <p className="mt-1 text-sm text-secondary">
                    {strings.quoteStudioHeaderArrangementHelp}
                  </p>
                </div>
                <div className="mt-4 grid gap-4 sm:grid-cols-2">
                  {(["left", "right"] as const).map((alignment) => (
                    <button
                      key={alignment}
                      type="button"
                      aria-pressed={design.headerAlignment === alignment}
                      className={cx(
                        "group relative min-h-40 overflow-hidden rounded-2xl border !p-5 text-left transition-colors hover:border-accent hover:bg-accent-soft/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
                        design.headerAlignment === alignment
                          ? "border-accent bg-accent-soft/30"
                          : "border-default bg-surface",
                      )}
                      onClick={() =>
                        onChange((current) => ({
                          ...current,
                          headerAlignment: alignment,
                        }))
                      }
                    >
                      <span
                        className={cx(
                          "flex h-20 items-center justify-between gap-5 rounded-xl bg-raised px-5",
                          alignment === "right" && "flex-row-reverse",
                        )}
                        aria-hidden="true"
                      >
                        <span className="flex items-center gap-2.5">
                          <span className="size-9 rounded-lg border border-accent/20 bg-accent-soft" />
                          <span className="space-y-1.5">
                            <span className="block h-2 w-16 rounded-full bg-primary/20" />
                            <span className="block h-1.5 w-11 rounded-full bg-primary/10" />
                          </span>
                        </span>
                        <span className="space-y-1.5">
                          <span className="block h-1.5 w-10 rounded-full bg-primary/15" />
                          <span className="block h-1.5 w-14 rounded-full bg-accent/70" />
                        </span>
                      </span>
                      <span className="flex items-start justify-between gap-5 pt-5">
                        <span>
                          <strong className="block text-sm font-semibold text-primary">
                            {alignment === "left"
                              ? strings.quoteStudioLogoLeft
                              : strings.quoteStudioLogoRight}
                          </strong>
                          <small className="mt-1 block text-xs font-normal leading-relaxed text-secondary">
                            {alignment === "left"
                              ? strings.quoteStudioLogoLeftHelp
                              : strings.quoteStudioLogoRightHelp}
                          </small>
                        </span>
                        <span
                          className={cx(
                            "mt-0.5 grid size-6 shrink-0 place-items-center rounded-full border transition-colors",
                            design.headerAlignment === alignment
                              ? "border-accent bg-accent text-white"
                              : "border-default bg-surface group-hover:border-accent",
                          )}
                        >
                          {design.headerAlignment === alignment && (
                            <Check className="size-3.5" />
                          )}
                        </span>
                      </span>
                    </button>
                  ))}
                </div>
              </section>
              <section className="border-t border-subtle pt-7">
                <div>
                  <h3 className="text-base font-semibold text-primary">
                    {strings.quoteStudioColumnBalance}
                  </h3>
                  <p className="mt-1 text-sm text-secondary">
                    {strings.quoteStudioColumnBalanceHelp}
                  </p>
                </div>
                <div
                  className="mt-6 grid grid-cols-1 gap-4 sm:grid-cols-3"
                  role="radiogroup"
                  aria-label={strings.quoteStudioColumnBalanceA11y}
                >
                  {headerRatioChoices.map((choice) => {
                    const selected = design.headerRatio === choice.id;
                    const [company = "50", customer = "50"] =
                      choice.id.split("-");
                    return (
                      <button
                        key={choice.id}
                        type="button"
                        role="radio"
                        aria-checked={selected}
                        aria-label={strings.quoteStudioColumnRatioA11y(
                          company,
                          customer,
                        )}
                        className={cx(
                          "group relative rounded-2xl p-3 transition-colors hover:bg-accent-soft/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
                          selected && "bg-accent-soft/40",
                        )}
                        onClick={() =>
                          onChange((current) => ({
                            ...current,
                            headerRatio: choice.id,
                          }))
                        }
                      >
                        <span
                          className={cx(
                            "grid h-24 gap-2 rounded-xl bg-raised p-3",
                            design.headerAlignment === "left"
                              ? choice.columns
                              : choice.reverseColumns,
                          )}
                          aria-hidden="true"
                        >
                          <span
                            className={cx(
                              "flex items-center justify-center rounded-lg bg-surface text-primary",
                              design.headerAlignment === "right" && "order-2",
                              selected && "ring-1 ring-accent/30",
                            )}
                          >
                            <Building2 className="size-6" strokeWidth={1.7} />
                          </span>
                          <span
                            className={cx(
                              "flex items-center justify-center rounded-lg bg-surface text-accent",
                              design.headerAlignment === "right" && "order-1",
                              selected && "ring-1 ring-accent/30",
                            )}
                          >
                            <ContactRound
                              className="size-6"
                              strokeWidth={1.7}
                            />
                          </span>
                        </span>
                        <span
                          className={cx(
                            "absolute right-2 top-2 grid size-6 place-items-center rounded-full border",
                            selected
                              ? "border-accent bg-accent text-white"
                              : "border-default bg-surface group-hover:border-accent",
                          )}
                          aria-hidden="true"
                        >
                          {selected && <Check className="size-3" />}
                        </span>
                      </button>
                    );
                  })}
                </div>
              </section>
            </>
          )}
          {mode === "document" && (
            <>
              <section className="rounded-2xl border border-subtle bg-surface p-6 shadow-sm sm:p-8">
                <div className="flex flex-wrap items-start justify-between gap-4">
                  <div>
                    <h3 className="text-2xl font-semibold tracking-tight text-primary">
                      {strings.quoteStudioDocumentPalette}
                    </h3>
                    <p className="mt-2 text-base text-secondary">
                      {strings.quoteStudioDocumentPaletteHelp}
                    </p>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      variant="secondary"
                      size="sm"
                      icon={<Palette aria-hidden="true" />}
                      onClick={() =>
                        onChange((current) => ({
                          ...current,
                          colors: importBrandQuoteColors(readBrandKit(), current.colors),
                        }))
                      }
                    >
                      {strings.quoteStudioImportBrandColors}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      icon={<RotateCcw aria-hidden="true" />}
                      onClick={() =>
                        onChange((current) => ({
                          ...current,
                            colors: DEFAULT_QUOTE_COLORS,
                        }))
                      }
                    >
                      {strings.quoteStudioResetDefaults}
                    </Button>
                  </div>
                </div>
                <div className="mt-8 grid gap-8 xl:grid-cols-2 xl:gap-0">
                  <div>
                    <div className="mb-6 flex items-center gap-4">
                      <span className="grid size-12 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
                        <FileText className="size-5" aria-hidden="true" />
                      </span>
                      <div>
                        <h4 className="text-base font-semibold text-primary">
                          {strings.quoteStudioDocument}
                        </h4>
                        <p className="mt-1 text-sm text-secondary">
                          {strings.quoteStudioDocumentHelp}
                        </p>
                      </div>
                    </div>
                    <div className="grid gap-3">
                      <ColorField
                        label={strings.quoteStudioAccent}
                        help={strings.quoteStudioAccentHelp}
                        value={design.colors.accent}
                        onChange={(value) => setColor("accent", value)}
                      />
                      <ColorField
                        label={strings.quoteStudioContactIcons}
                        help={strings.quoteStudioContactIconsHelp}
                        value={design.colors.contactIcons}
                        onChange={(value) => setColor("contactIcons", value)}
                      />
                      <ColorField
                        label={strings.quoteStudioPage}
                        help={strings.quoteStudioPageHelp}
                        value={design.colors.background}
                        onChange={(value) => setColor("background", value)}
                      />
                      <ColorField
                        label={strings.quoteStudioHeader}
                        help={strings.quoteStudioHeaderHelp}
                        value={design.colors.headerBackground}
                        onChange={(value) =>
                          setColor("headerBackground", value)
                        }
                      />
                      <ColorField
                        label={strings.quoteStudioText}
                        help={strings.quoteStudioTextHelp}
                        value={design.colors.text}
                        onChange={(value) => setColor("text", value)}
                      />
                      <ColorField
                        label={strings.quoteStudioBulletDots}
                        help={strings.quoteStudioListMarkers}
                        value={design.colors.bulletMarker}
                        onChange={(value) => setColor("bulletMarker", value)}
                      />
                      <ColorField
                        label={strings.quoteStudioNumberMarkers}
                        help={strings.quoteStudioNumberedSteps}
                        value={design.colors.numberMarker}
                        onChange={(value) => setColor("numberMarker", value)}
                      />
                    </div>
                  </div>
                  <div className="border-t border-subtle pt-8 xl:border-l xl:border-t-0 xl:pl-8 xl:pt-0">
                    <div className="mb-6 flex items-center gap-4">
                      <span className="grid size-12 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
                        <Table2 className="size-5" aria-hidden="true" />
                      </span>
                      <div>
                        <h4 className="text-base font-semibold text-primary">
                          {strings.quoteStudioPricingTables}
                        </h4>
                        <p className="mt-1 text-sm text-secondary">
                          {strings.quoteStudioPricingTablesHelp}
                        </p>
                      </div>
                    </div>
                    <div className="grid gap-3">
                      <ColorField
                        label={strings.quoteStudioTableHeading}
                        help={strings.quoteStudioTableHeadingHelp}
                        value={design.colors.tableHeader}
                        onChange={(value) => setColor("tableHeader", value)}
                      />
                      <ColorField
                        label={strings.quoteStudioTableRows}
                        help={strings.quoteStudioTableRowsHelp}
                        value={design.colors.tableRows}
                        onChange={(value) => setColor("tableRows", value)}
                      />
                    </div>
                  </div>
                </div>
              </section>
              <section className="border-t border-subtle pt-7">
                <div className="flex flex-wrap items-start justify-between gap-4">
                  <div>
                  <h3 className="text-base font-semibold text-primary">
                    {strings.quoteStudioTypography}
                  </h3>
                  <p className="mt-1 text-sm text-secondary">
                    {strings.quoteStudioTypographyHelp}
                  </p>
                  </div>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => onChange((current) => ({ ...current, ...importBrandQuoteTypography(readBrandKit()) }))}
                  >
                    {strings.quoteStudioImportBrandTypography}
                  </Button>
                </div>
                <div className="mt-5 grid gap-4 sm:grid-cols-3">
                  {themeChoices.map((theme) => (
                    <button
                      key={theme.id}
                      type="button"
                      aria-pressed={design.theme === theme.id}
                      className={cx(
                        "group relative min-h-52 overflow-hidden rounded-2xl border !p-4 text-left transition-colors hover:border-accent hover:bg-accent-soft/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
                        design.theme === theme.id
                          ? "border-accent bg-accent-soft/30"
                          : "border-default bg-surface",
                      )}
                      onClick={() =>
                        onChange((current) => ({ ...current, theme: theme.id, ...themeQuoteTypography(theme.id) }))
                      }
                    >
                      <span
                        className={cx(
                          "block h-28 rounded-xl border border-subtle bg-raised px-4 py-4",
                        )}
                        aria-hidden="true"
                      >
                        <span
                          className={cx(
                            "block text-xl leading-none text-primary",
                            theme.id === "modern" &&
                              "font-semibold tracking-tight",
                            theme.id === "editorial" && "font-editorial",
                            theme.id === "minimal" &&
                              "font-light uppercase tracking-[0.14em]",
                          )}
                        >
                          {strings.quoteStudioProposal}
                        </span>
                        <span
                          className={cx(
                            "mt-4 block h-1.5 rounded-full bg-primary/20",
                            theme.id === "modern" && "w-4/5",
                            theme.id === "editorial" && "w-full",
                            theme.id === "minimal" && "w-3/5",
                          )}
                        />
                        <span className="mt-2 block h-1.5 w-2/3 rounded-full bg-primary/10" />
                      </span>
                      <span className="flex items-start justify-between gap-3 px-1 pb-1 pt-4">
                        <span>
                          <strong className="block text-sm font-semibold text-primary">
                            {theme.name}
                          </strong>
                          <small className="mt-1 block text-xs leading-relaxed text-secondary">
                            {theme.help}
                          </small>
                        </span>
                        <span
                          className={cx(
                            "grid size-6 shrink-0 place-items-center rounded-full border transition-colors",
                            design.theme === theme.id
                              ? "border-accent bg-accent text-on-accent"
                              : "border-default bg-surface group-hover:border-accent",
                          )}
                        >
                          {design.theme === theme.id && (
                            <Check className="size-3.5" aria-hidden="true" />
                          )}
                        </span>
                      </span>
                    </button>
                  ))}
                </div>
              </section>
            </>
          )}
        </div>
      </div>
    </Modal>
  );
}
