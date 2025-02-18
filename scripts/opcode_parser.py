from bs4 import BeautifulSoup
import re

# Open and read the HTML file containing the opcode table.
with open('opcode_table.html', 'r', encoding='utf-8') as f:
    soup = BeautifulSoup(f, 'html.parser')

def parse_opcode_cell(cell):
    text = cell.get_text(separator='\n').strip()
    lines = text.split('\n')
    if not lines or lines[0].strip() == "":
        # Empty cell
        return None
    mnemonic = lines[0].strip()
    # Check if there is a second line with length and cycle info.
    if len(lines) < 2:
        return mnemonic, 1, 0
    match = re.search(r"(\d+)\s+(\d+)t", lines[1])
    if match:
        length = int(match.group(1))
        cycles = int(match.group(2))
    else:
        length, cycles = 1, 0
    return mnemonic, length, cycles

# Parse the unprefixed opcodes.
unprefixed_table = soup.find('table', id='unprefixed-16-t')
unprefixed_instructions = []

for cell in unprefixed_table.find_all('td'):
    index_str = cell.get('data-index')
    if not index_str:
        continue
    idx = int(index_str)
    parsed = parse_opcode_cell(cell)
    if parsed is None:
        continue  # Skip empty entries.
    mnemonic, length, cycles = parsed
    unprefixed_instructions.append((idx, mnemonic, length, cycles))

# Sort the instructions by opcode index.
unprefixed_instructions.sort(key=lambda x: x[0])

# Parse the CB-prefixed opcodes.
cb_table = soup.find('table', id='cbprefixed-16-t')
cb_instructions = []

for cell in cb_table.find_all('td'):
    index_str = cell.get('data-index')
    if not index_str:
        continue
    idx = int(index_str)
    parsed = parse_opcode_cell(cell)
    if parsed is None:
        continue
    mnemonic, length, cycles = parsed
    cb_instructions.append((idx, mnemonic, length, cycles))

cb_instructions.sort(key=lambda x: x[0])

# Print statistics.
print("Unprefixed opcodes found:", len(unprefixed_instructions))
print("CB-prefixed opcodes found:", len(cb_instructions))
print("Total:", (len(unprefixed_instructions)+len(cb_instructions)))

# Generate Rust file lines.
rust_lines = []
rust_lines.append("pub struct Instruction {")
rust_lines.append("    pub name: &'static str,")
rust_lines.append("    pub operation: fn(&mut super::Cpu, &mut super::Bus) -> u32,")
rust_lines.append("    pub length: u8,")
rust_lines.append("    pub cycles: u32,")
rust_lines.append("}\n")

rust_lines.append(f"pub const INSTRUCTION_TABLE: [Instruction; {len(unprefixed_instructions)}] = [")
for idx, mnemonic, length, cycles in unprefixed_instructions:
    rust_lines.append(f"    // 0x{idx:02X} - {mnemonic}")
    rust_lines.append("    Instruction {")
    rust_lines.append(f'        name: "{mnemonic}",')
    # A basic stub for operation that simply returns the cycle count.
    rust_lines.append(f"        operation: |_cpu, _bus| {cycles},")
    rust_lines.append(f"        length: {length},")
    rust_lines.append(f"        cycles: {cycles},")
    rust_lines.append("    },")
rust_lines.append("];\n")

rust_lines.append(f"pub const CB_INSTRUCTION_TABLE: [Instruction; {len(cb_instructions)}] = [")
for idx, mnemonic, length, cycles in cb_instructions:
    rust_lines.append(f"    // CB 0x{idx:02X} - {mnemonic}")
    rust_lines.append("    Instruction {")
    rust_lines.append(f'        name: "{mnemonic}",')
    rust_lines.append(f"        operation: |_cpu, _bus| {cycles},")
    rust_lines.append(f"        length: {length},")
    rust_lines.append(f"        cycles: {cycles},")
    rust_lines.append("    },")
rust_lines.append("];\n")

# Write the generated Rust code to a file.
with open('opcodes.rs', 'w', encoding='utf-8') as out:
    out.write("\n".join(rust_lines))

print("Rust opcode file generated as opcodes.rs")
