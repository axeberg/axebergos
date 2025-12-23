# Man page build process
#
# Requires: scdoc, groff
# Install: apt-get install scdoc groff
#
# Source files: foo.1.scd
# Output: compiled/foo.1 (troff), formatted/foo.txt (plain text)

SCDOC_SOURCES := $(wildcard *.1.scd)
NAMES := $(patsubst %.1.scd,%,$(SCDOC_SOURCES))
TROFF_FILES := $(patsubst %,compiled/%.1,$(NAMES))
TEXT_FILES := $(patsubst %,formatted/%.txt,$(NAMES))

.PHONY: all clean troff text

all: text

# Compile scdoc to troff format
troff: $(TROFF_FILES)

compiled/%.1: %.1.scd | compiled
	scdoc < $< > $@

# Render troff to plain text
text: $(TEXT_FILES)

formatted/%.txt: compiled/%.1 | formatted
	groff -man -Tascii $< | sed -e 's/.\x08//g' -e 's/\x1b\[[0-9;]*m//g' > $@

compiled:
	mkdir -p compiled

formatted:
	mkdir -p formatted

clean:
	rm -rf compiled formatted

# Rebuild all man pages
rebuild: clean all
