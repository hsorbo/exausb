// SPDX-License-Identifier: GPL-2.0-or-later
/*
 * Differential-testing driver for upstream usbutils' lsusb.c.
 *
 * Includes the unmodified lsusb.c (with its main() renamed), stubs out the
 * udev/sysfs dependent helpers, and dumps synthetic descriptors read from
 * stdin.  The input format is the one documented in examples/difftest.rs.
 * Name database lookups (udev hwdb) always fail, matching the Rust side
 * when no usb.ids is loaded.
 */
#define main lsusb_main
#include "lsusb.c"
#undef main

#include "usb-spec.h"

/* ---- names.c, without udev ------------------------------------------- */

static const char *names_genericstrtable(const struct genericstrtable *t, unsigned int idx)
{
	for (; t->name; t++)
		if (t->num == idx)
			return t->name;
	return NULL;
}
const char *names_hid(uint8_t hidd) { return names_genericstrtable(hiddescriptors, hidd); }
const char *names_reporttag(uint8_t rt) { return names_genericstrtable(reports, rt); }
const char *names_huts(unsigned int data) { return names_genericstrtable(huts, data); }
const char *names_hutus(unsigned int data) { return names_genericstrtable(hutus, data); }
const char *names_langid(uint16_t langid) { return names_genericstrtable(langids, langid); }
const char *names_physdes(uint8_t ph) { return names_genericstrtable(physdess, ph); }
const char *names_bias(uint8_t b) { return names_genericstrtable(biass, b); }
const char *names_countrycode(unsigned int c) { return names_genericstrtable(countrycodes, c); }
const char *names_vendor(uint16_t v) { (void)v; return NULL; }
const char *names_product(uint16_t v, uint16_t p) { (void)v; (void)p; return NULL; }
const char *names_class(uint8_t c) { (void)c; return NULL; }
const char *names_subclass(uint8_t c, uint8_t s) { (void)c; (void)s; return NULL; }
const char *names_protocol(uint8_t c, uint8_t s, uint8_t p) { (void)c; (void)s; (void)p; return NULL; }
const char *names_audioterminal(uint16_t termt)
{
	const struct audioterminal *at;
	for (at = audioterminals; at->name; at++)
		if (at->termt == termt)
			return at->name;
	return NULL;
}
const char *names_videoterminal(uint16_t termt)
{
	const struct videoterminal *vt;
	for (vt = videoterminals; vt->name; vt++)
		if (vt->termt == termt)
			return vt->name;
	return NULL;
}
int get_vendor_string(char *buf, size_t size, uint16_t vid) { (void)vid; if (size) *buf = 0; return 0; }
int get_product_string(char *buf, size_t size, uint16_t v, uint16_t p) { (void)v; (void)p; if (size) *buf = 0; return 0; }
int get_class_string(char *buf, size_t size, uint8_t cls) { (void)cls; return snprintf(buf, size, "[unknown]"); }
int get_subclass_string(char *buf, size_t size, uint8_t c, uint8_t s) { (void)c; (void)s; return snprintf(buf, size, "[unknown]"); }
void get_vendor_product_with_fallback(char *v, int vl, char *p, int pl, libusb_device *d)
{ (void)d; snprintf(v, vl, "[unknown]"); snprintf(p, pl, "[unknown]"); }
int names_init(void) { return 0; }
void names_exit(void) { }

/* ---- sysfs.c / usbmisc.c / lsusb-t.c ----------------------------------- */

int get_sysfs_name(char *buf, size_t size, libusb_device *dev) { (void)dev; if (size) *buf = 0; return -1; }
int read_sysfs_prop(char *buf, size_t size, const char *n, const char *p) { (void)n; (void)p; if (size) *buf = 0; return 0; }
libusb_device *get_usb_device(libusb_context *ctx, const char *path) { (void)ctx; (void)path; return NULL; }
char *get_dev_string(libusb_device_handle *dev, uint8_t id) { (void)dev; (void)id; return strdup(""); }
int lsusb_t(void) { return 0; }

/* ---- driver ---------------------------------------------------------- */

static unsigned hexnum(const char *s) { return (unsigned)strtoul(s, NULL, 16); }

/* Parse hex bytes into a zero-padded heap buffer; returns length. */
static unsigned char *hexbytes(const char *s, int *len)
{
	size_t n = strcmp(s, "-") ? strlen(s) / 2 : 0;
	unsigned char *b = calloc(1, n + 1024);
	for (size_t i = 0; i < n; i++) {
		char t[3] = { s[2 * i], s[2 * i + 1], 0 };
		b[i] = (unsigned char)strtoul(t, NULL, 16);
	}
	*len = (int)n;
	return b;
}

/* Same loop as dump_bos_descriptor(), minus the device I/O. */
static void dump_caps(unsigned char *buf, int size, bool lpm_required)
{
	bool has_ssp = false;
	while (size >= 3) {
		if (buf[0] < 3 || buf[0] > size) {
			printf("  ** Bad device-capability bLength %u (%d left)\n", buf[0], size);
			return;
		}
		switch (buf[2]) {
		case USB_DC_WIRELESS_USB: break;
		case USB_DC_20_EXTENSION: dump_usb2_device_capability_desc(buf, lpm_required); break;
		case USB_DC_SUPERSPEED: dump_ss_device_capability_desc(buf); break;
		case USB_DC_SUPERSPEEDPLUS: dump_ssp_device_capability_desc(buf); has_ssp = true; break;
		case USB_DC_CONTAINER_ID: dump_container_id_device_capability_desc(buf); break;
		case USB_DC_PLATFORM: dump_platform_device_capability_desc(NULL, buf); break;
		case USB_DC_BILLBOARD: dump_billboard_device_capability_desc(NULL, buf); break;
		case USB_DC_BILLBOARD_ALT_MODE: dump_billboard_alt_mode_capability_desc(buf); break;
		case USB_DC_CONFIGURATION_SUMMARY:
			printf("  Configuration Summary Device Capability:\n");
			desc_dump(NULL, desc_usb3_dc_configuration_summary, buf, DESC_BUF_LEN_FROM_BUF, 2);
			break;
		case USB_DC_FWSTATUS_CAPABILITY: dump_fwstatus_capability_desc(buf); break;
		default:
			printf("  ** UNRECOGNIZED: ");
			dump_bytes(buf, buf[0]);
			break;
		}
		size -= buf[0];
		buf += buf[0];
	}
	(void)has_ssp;
}

int main(void)
{
	static char line[1 << 16];
	struct libusb_config_descriptor *cfg = NULL;
	struct libusb_interface *ifs = NULL;
	struct libusb_interface_descriptor *alts = NULL;
	unsigned speed = 0;
	int n = -1;

	setvbuf(stdout, NULL, _IOFBF, 1 << 16);
	while (fgets(line, sizeof(line), stdin)) {
		char *f[8] = { 0 };
		int nf = 0, len;
		n++;
		for (char *t = strtok(line, " \t\n"); t && nf < 8; t = strtok(NULL, " \t\n"))
			f[nf++] = t;
		if (!nf)
			continue;
		if (strcmp(f[0], "IFACE") && strcmp(f[0], "EP") && strcmp(f[0], "END"))
			printf("#### record %d\n", n);

		if (!strcmp(f[0], "CONFIG")) {
			cfg = calloc(1, sizeof(*cfg));
			ifs = calloc(64, sizeof(*ifs));
			alts = calloc(64, sizeof(*alts));
			speed = hexnum(f[1]);
			cfg->bLength = 9;
			cfg->bDescriptorType = 2;
			cfg->bConfigurationValue = 1;
			cfg->bmAttributes = hexnum(f[2]);
			cfg->MaxPower = hexnum(f[3]);
			cfg->iConfiguration = hexnum(f[4]);
			cfg->extra = hexbytes(f[5], &len);
			cfg->extra_length = len;
			cfg->interface = ifs;
		} else if (!strcmp(f[0], "IFACE")) {
			struct libusb_interface_descriptor *a = &alts[cfg->bNumInterfaces];
			a->bLength = 9;
			a->bDescriptorType = 4;
			a->bInterfaceClass = hexnum(f[1]);
			a->bInterfaceSubClass = hexnum(f[2]);
			a->bInterfaceProtocol = hexnum(f[3]);
			a->bInterfaceNumber = hexnum(f[4]);
			a->bAlternateSetting = hexnum(f[5]);
			a->extra = hexbytes(f[6], &len);
			a->extra_length = len;
			a->endpoint = calloc(32, sizeof(struct libusb_endpoint_descriptor));
			ifs[cfg->bNumInterfaces].altsetting = a;
			ifs[cfg->bNumInterfaces].num_altsetting = 1;
			cfg->bNumInterfaces++;
		} else if (!strcmp(f[0], "EP")) {
			struct libusb_interface_descriptor *a = &alts[cfg->bNumInterfaces - 1];
			struct libusb_endpoint_descriptor *e =
				(struct libusb_endpoint_descriptor *)&a->endpoint[a->bNumEndpoints++];
			e->bLength = hexnum(f[1]);
			e->bDescriptorType = 5;
			e->bEndpointAddress = hexnum(f[2]);
			e->bmAttributes = hexnum(f[3]);
			e->wMaxPacketSize = hexnum(f[4]);
			e->bInterval = hexnum(f[5]);
			e->extra = hexbytes(f[6], &len);
			e->extra_length = len;
		} else if (!strcmp(f[0], "END")) {
			dump_config(NULL, cfg, speed);
		} else if (!strcmp(f[0], "HUB")) {
			unsigned char *b = hexbytes(f[2], &len);
			dump_hub("", b, hexnum(f[1]));
		} else if (!strcmp(f[0], "REPORT")) {
			unsigned char *b = hexbytes(f[1], &len);
			dump_report_desc(b, len);
		} else if (!strcmp(f[0], "BOS")) {
			unsigned char *b = hexbytes(f[2], &len);
			dump_caps(b, len, hexnum(f[1]) != 0);
		}
		fflush(stdout);
	}
	return 0;
}
