import { describe, expect, it } from "bun:test";
import { parseQuery } from "../src/domain/query";

describe("parseQuery", () => {
  describe("field-prefixed quoted values", () => {
    it('parses title:"my task" as a single term', () => {
      const result = parseQuery('title:"my task"');
      expect(result.terms).toHaveLength(1);
      expect(result.terms[0]).toEqual({
        field: "title",
        value: "my task",
        negated: false,
      });
    });

    it('parses status:"in progress" as a single term', () => {
      const result = parseQuery('status:"in progress"');
      expect(result.terms).toHaveLength(1);
      expect(result.terms[0]).toEqual({
        field: "status",
        value: "in progress",
        negated: false,
      });
    });

    it("parses negated field-prefixed quoted value", () => {
      const result = parseQuery('-title:"my task"');
      expect(result.terms).toHaveLength(1);
      expect(result.terms[0]).toEqual({
        field: "title",
        value: "my task",
        negated: true,
      });
    });

    it("parses mixed field-prefixed quoted and unquoted terms", () => {
      const result = parseQuery('status:open title:"my task"');
      expect(result.terms).toHaveLength(2);
      expect(result.terms[0]).toEqual({
        field: "status",
        value: "open",
        negated: false,
      });
      expect(result.terms[1]).toEqual({
        field: "title",
        value: "my task",
        negated: false,
      });
    });

    it("parses field-prefixed quoted value followed by bare words", () => {
      const result = parseQuery('title:"my task" login');
      expect(result.terms).toHaveLength(2);
      expect(result.terms[0]).toEqual({
        field: "title",
        value: "my task",
        negated: false,
      });
      expect(result.terms[1]).toEqual({
        field: "text",
        value: "login",
        negated: false,
      });
    });

    it("parses bare words followed by field-prefixed quoted value", () => {
      const result = parseQuery('login title:"my task"');
      expect(result.terms).toHaveLength(2);
      expect(result.terms[0]).toEqual({
        field: "text",
        value: "login",
        negated: false,
      });
      expect(result.terms[1]).toEqual({
        field: "title",
        value: "my task",
        negated: false,
      });
    });

    it('parses label:"multi word" as a single term', () => {
      const result = parseQuery('label:"high priority"');
      expect(result.terms).toHaveLength(1);
      expect(result.terms[0]).toEqual({
        field: "label",
        value: "high priority",
        negated: false,
      });
    });
  });

  describe("basic tokenization", () => {
    it("parses simple bare word as text term", () => {
      const result = parseQuery("login");
      expect(result.terms).toHaveLength(1);
      expect(result.terms[0]).toEqual({
        field: "text",
        value: "login",
        negated: false,
      });
    });

    it("parses simple field:value as field term", () => {
      const result = parseQuery("status:open");
      expect(result.terms).toHaveLength(1);
      expect(result.terms[0]).toEqual({
        field: "status",
        value: "open",
        negated: false,
      });
    });

    it("parses standalone quoted phrase as single text term", () => {
      const result = parseQuery('"fix login bug"');
      expect(result.terms).toHaveLength(1);
      expect(result.terms[0]).toEqual({
        field: "text",
        value: "fix login bug",
        negated: false,
      });
    });

    it("returns empty terms for empty query", () => {
      const result = parseQuery("");
      expect(result.terms).toHaveLength(0);
    });

    it("returns empty terms for whitespace-only query", () => {
      const result = parseQuery("   ");
      expect(result.terms).toHaveLength(0);
    });

    it("combines consecutive bare words into a single text term", () => {
      const result = parseQuery("fix login bug");
      expect(result.terms).toHaveLength(1);
      expect(result.terms[0]).toEqual({
        field: "text",
        value: "fix login bug",
        negated: false,
      });
    });
  });

  describe("dependency query fields", () => {
    it("parses dep_type_in and dep_type_out fielded terms", () => {
      const result = parseQuery("dep_type_in:blocks dep_type_out:starts_after");
      expect(result.terms).toHaveLength(2);
      expect(result.terms[0]).toEqual({
        field: "dep_type_in",
        value: "blocks",
        negated: false,
      });
      expect(result.terms[1]).toEqual({
        field: "dep_type_out",
        value: "starts_after",
        negated: false,
      });
    });

    it("rejects ambiguous dep_type field without direction", () => {
      expect(() => parseQuery("dep_type:blocks")).toThrow();
    });

    it("rejects invalid dependency type values for dep_type_in/out", () => {
      expect(() => parseQuery("dep_type_in:unknown")).toThrow();
      expect(() => parseQuery("dep_type_out:unknown")).toThrow();
    });
  });
});
